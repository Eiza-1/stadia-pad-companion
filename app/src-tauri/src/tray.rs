//! Tray icon: open, live status line, Start with Windows, Quit.

use crate::{open_window, quit};
use pad_core::bridge::{Status, Transport};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub struct TrayItems {
    status: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

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

pub fn build(app: &AppHandle, start_with_windows: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Pad Companion", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "Controller not connected", false, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, start_with_windows, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&open, &sep()?, &status, &sep()?, &autostart, &sep()?, &quit_item])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundled icon"))
        .tooltip("Pad Companion")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => open_window(app),
            "quit" => quit(app),
            "autostart" => {
                let state = app.state::<crate::AppState>();
                if let Some(mut s) = state.with_bridge(|b| b.settings()) {
                    s.start_with_windows = !s.start_with_windows;
                    let _ = crate::ipc::set_settings(app.clone(), state.clone(), s);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                open_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayItems { status, autostart });
    Ok(())
}

pub fn set_status(app: &AppHandle, s: &Status) {
    if let Some(t) = app.try_state::<TrayItems>() {
        let text = status_text(s);
        let _ = t.status.set_text(&text);
        if let Some(icon) = app.tray_by_id("main") {
            let _ = icon.set_tooltip(Some(format!("Pad Companion — {text}")));
        }
    }
}

pub fn set_autostart_checked(app: &AppHandle, on: bool) {
    if let Some(t) = app.try_state::<TrayItems>() {
        let _ = t.autostart.set_checked(on);
    }
}
