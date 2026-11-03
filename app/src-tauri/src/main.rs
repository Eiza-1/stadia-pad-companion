#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Pad Companion: tray app around the always-on controller bridge. The settings window is created on
//! demand and destroyed on close, so WebView2 costs nothing while you play.

mod ipc;
mod overlay;
mod tray;

use pad_core::bridge::{Bridge, BridgeEvent, BridgeHandle};
use pad_core::settings::Settings;
use pad_core::special::Action;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub struct AppState {
    bridge: Mutex<Option<BridgeHandle>>,
    pub settings_path: PathBuf,
}

impl AppState {
    pub fn with_bridge<T>(&self, f: impl FnOnce(&BridgeHandle) -> T) -> Option<T> {
        self.bridge.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(f)
    }
}

/// Shows the window, creating it if needed. Safe to call from any thread.
pub fn open_window(app: &AppHandle) {
    open_window_at(app, None);
}

/// Shows the window on a configuration page (e.g. "other" for Settings), or as it was when `None`.
pub fn open_window_at(app: &AppHandle, page: Option<&'static str>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            if let Some(p) = page {
                let _ = w.emit("goto", p);
            }
            return;
        }
        let url = match page {
            Some(p) => format!("index.html#{p}"),
            None => "index.html".into(),
        };
        let built = WebviewWindowBuilder::new(&handle, "main", WebviewUrl::App(url.into()))
            .title("Pad Companion")
            .decorations(false)
            .inner_size(1280.0, 800.0)
            .min_inner_size(1100.0, 700.0)
            .center()
            .background_color(tauri::window::Color(14, 15, 18, 255))
            .build();
        match built {
            Ok(w) => {
                // Live pad/stats events start only when the UI asks (ipc::set_live: a configuration
                // page is open and the window isn't minimised) and stop when the window goes.
                let h = handle.clone();
                w.on_window_event(move |e| {
                    if let WindowEvent::Destroyed = e {
                        h.state::<AppState>().with_bridge(|b| b.set_live(false));
                    }
                });
            }
            Err(e) => pad_core::log!("window create failed: {e}"),
        }
    });
}

/// Tray Quit: stop the bridge (removes the virtual pad, un-hides the real one), then exit.
pub fn quit(app: &AppHandle) {
    let bridge = app.state::<AppState>().bridge.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(b) = bridge {
        b.stop();
    }
    app.exit(0);
}

/// Forwards bridge events: status to the tray always; live data to the window only while it exists.
fn spawn_dispatcher(app: AppHandle, rx: mpsc::Receiver<BridgeEvent>) {
    std::thread::Builder::new()
        .name("ui-dispatch".into())
        .spawn(move || {
            for ev in rx {
                let window = app.get_webview_window("main");
                match ev {
                    BridgeEvent::Status(s) => {
                        tray::set_status(&app, &s);
                        if s.connection.is_none() {
                            overlay::on_pad_gone(&app);
                        }
                        if let Some(w) = window {
                            let _ = w.emit("status", s);
                        }
                    }
                    BridgeEvent::Action(Action::OpenCompanion) => open_window(&app),
                    BridgeEvent::Action(a @ (Action::OverlayTap | Action::TalkStart | Action::TalkEnd | Action::LiveStart)) => {
                        overlay::on_action(&app, a)
                    }
                    BridgeEvent::Action(a) => {
                        if let Some(w) = window {
                            let _ = w.emit("action", a);
                        }
                    }
                    BridgeEvent::Pad { state, out } => {
                        if let Some(w) = window {
                            let _ = w.emit("pad", serde_json::json!({ "state": state, "out": out }));
                        }
                    }
                    BridgeEvent::Captured(state) => {
                        if let Some(w) = app.get_webview_window("overlay") {
                            let _ = w.emit("pad", state);
                        }
                    }
                    BridgeEvent::Stats(s) => {
                        if let Some(w) = window {
                            let _ = w.emit("stats", s);
                        }
                    }
                }
            }
        })
        .expect("spawn dispatcher");
}

fn main() {
    // Uninstaller hook: give the pad back and remove autostart, without starting the app.
    if std::env::args().any(|a| a == "--cleanup") {
        pad_core::win::autostart::set(false);
        pad_core::win::hidhide::cleanup();
        return;
    }
    let background = std::env::args().any(|a| a == "--background");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_window(app)))
        .invoke_handler(tauri::generate_handler![
            ipc::get_settings,
            ipc::set_settings,
            ipc::get_status,
            ipc::test_rumble,
            ipc::app_info,
            ipc::open_external,
            ipc::capture_info,
            ipc::open_log_folder,
            ipc::window_minimize,
            ipc::window_maximize,
            ipc::window_close,
            ipc::set_live,
            ipc::overlay_reset_pos,
            ipc::set_overlay_hotkey,
            ipc::hotkey_label,
            ipc::gemini_key_status,
            ipc::gemini_key_set,
            ipc::gemini_key_clear,
            ipc::gemini_open_studio,
            overlay::overlay_ready,
            overlay::overlay_resize,
            overlay::overlay_style,
            overlay::overlay_hush,
            overlay::gemini_voices,
            overlay::gemini_voice_sample,
            overlay::overlay_ask,
            overlay::overlay_capture,
            overlay::overlay_close,
            overlay::overlay_focus,
            overlay::overlay_show_type,
            overlay::overlay_drag_end,
            tray::tray_action,
            tray::tray_ready,
            tray::tray_set_autostart,
        ])
        .setup(move |app| {
            let settings_path = Settings::default_path();
            let settings = Settings::load(&settings_path);
            // Re-apply every start: an upgrade runs the old uninstaller (which removes the Run value),
            // and a moved install changes the exe path.
            pad_core::win::autostart::set(settings.start_with_windows);
            let bridge = Bridge::start(settings);
            bridge.set_live(false);
            let (tx, rx) = mpsc::channel();
            bridge.subscribe(tx);
            app.manage(AppState { bridge: Mutex::new(Some(bridge)), settings_path });
            app.manage(overlay::Overlay::default());
            // Overlay keyboard shortcut (tap = type, hold = talk), registered with Windows.
            let h = app.handle().clone();
            let keys = pad_core::win::hotkey_win::HotkeyThread::start(move |a| overlay::on_key_action(&h, a));
            let wanted = app.state::<AppState>().with_bridge(|b| b.settings().overlay_hotkey).flatten();
            if let Err(e) = keys.set(wanted) {
                pad_core::log!("overlay shortcut not registered: {e}");
            }
            app.manage(keys);
            spawn_dispatcher(app.handle().clone(), rx);
            tray::build(app.handle())?;
            if !background {
                open_window(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Pad Companion");

    app.run(|app, event| match event {
        // Closing the window must not end the app: the controller bridge lives in the tray.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            let bridge = app.state::<AppState>().bridge.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(b) = bridge {
                b.stop();
            }
        }
        _ => {}
    });
}
