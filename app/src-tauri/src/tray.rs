//! Tray icon. Left-click opens Pad Companion; right-click opens a small custom menu (tray.html): the
//! pad's photo, name, connection and battery ring, then Open, Settings, Start with Windows and Quit.
//!
//! The menu is a WebView window created on right-click and destroyed as soon as it loses focus, so it
//! costs nothing while you play.

use crate::{open_window, open_window_at, quit};
use pad_core::bridge::{Status, Transport};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const MENU: &str = "traymenu";
/// Menu size in logical pixels (tray.html is laid out for exactly this, shadow margin included).
const W: f64 = 316.0;
const H: f64 = 280.0;

pub fn status_text(s: &Status) -> String {
    if !s.vigem_ok {
        return "ViGEmBus driver missing".into();
    }
    match s.connection {
        None => "Controller not connected".into(),
        Some(Transport::Usb) => "Connected · USB cable".into(),
        Some(Transport::Bluetooth) => {
            let mut t = "Connected · Bluetooth".to_string();
            if let Some(b) = s.battery {
                t += &format!(" · {b}%");
            }
            t
        }
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundled icon"))
        .tooltip("Pad Companion")
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, position, .. } = e {
                match button {
                    MouseButton::Left => {
                        close_menu(tray.app_handle());
                        open_window(tray.app_handle());
                    }
                    MouseButton::Right => toggle_menu(tray.app_handle(), position),
                    _ => {}
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn close_menu(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MENU) {
        let _ = w.destroy();
    }
}

/// Opens the menu next to the cursor (above it with a bottom taskbar, below it with a top one), kept
/// inside the monitor's work area. A second right-click while it is open closes it.
fn toggle_menu(app: &AppHandle, at: PhysicalPosition<f64>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window(MENU) {
            let _ = w.destroy();
            return;
        }
        let monitor = handle.monitor_from_point(at.x, at.y).ok().flatten().or_else(|| handle.primary_monitor().ok().flatten());
        let (scale, area) = match &monitor {
            Some(m) => (m.scale_factor(), Some(*m.work_area())),
            None => (1.0, None),
        };
        let (w, h) = (W * scale, H * scale);
        let (mut x, mut y) = (at.x - w + 24.0 * scale, at.y - h - 6.0 * scale);
        if let Some(a) = area {
            let (l, t) = (a.position.x as f64, a.position.y as f64);
            let (r, b) = (l + a.size.width as f64, t + a.size.height as f64);
            if y < t {
                y = at.y + 6.0 * scale; // taskbar at the top of the screen
            }
            x = x.clamp(l + 4.0, (r - w - 4.0).max(l));
            y = y.clamp(t + 4.0, (b - h).max(t));
        }
        let built = WebviewWindowBuilder::new(&handle, MENU, WebviewUrl::App("tray.html".into()))
            .title("Pad Companion menu")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .inner_size(W, H)
            .position(x / scale, y / scale)
            .visible(false) // tray.js shows it once drawn, so it never flashes empty
            .build();
        match built {
            Ok(win) => {
                let h = handle.clone();
                win.on_window_event(move |e| {
                    if let WindowEvent::Focused(false) = e {
                        close_menu(&h);
                    }
                });
            }
            Err(e) => pad_core::log!("tray menu create failed: {e}"),
        }
    });
}

/// What the menu's buttons do.
#[tauri::command]
pub fn tray_action(app: AppHandle, action: String) {
    match action.as_str() {
        "open" => open_window(&app),
        "settings" => open_window_at(&app, Some("other")),
        "quit" => quit(&app),
        _ => {}
    }
    close_menu(&app);
}

/// The menu's Start with Windows switch. Saved here rather than by the menu page, and sent to the main
/// window, so its own copy of the settings can't later overwrite the change.
#[tauri::command]
pub fn tray_set_autostart(app: AppHandle, state: tauri::State<'_, crate::AppState>, on: bool) -> Result<(), String> {
    let mut s = crate::ipc::get_settings(state.clone());
    if s.start_with_windows == on {
        return Ok(());
    }
    s.start_with_windows = on;
    crate::ipc::set_settings(app.clone(), state, s)?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("autostart", on);
    }
    Ok(())
}

/// tray.js calls this once its content is drawn: show and focus (focus loss then closes it).
#[tauri::command]
pub fn tray_ready(window: tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

/// Status goes to the tooltip always, and to the menu while it is open.
pub fn set_status(app: &AppHandle, s: &Status) {
    let text = status_text(s);
    if let Some(icon) = app.tray_by_id("main") {
        let _ = icon.set_tooltip(Some(format!("Pad Companion — {text}")));
    }
    if let Some(w) = app.get_webview_window(MENU) {
        let _ = w.emit("status", s);
    }
}
