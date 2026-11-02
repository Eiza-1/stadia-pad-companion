//! Tauri commands the UI calls, and the mapping from bridge errors to UI status.

use crate::AppState;
use pad_core::bridge::{BridgeError, Status};
use pad_core::settings::Settings;
use pad_core::win::{actions, autostart};
use serde::Serialize;
use tauri::{State, WebviewWindow};

/// What the UI shows when the bridge could not start for a known reason.
pub fn status_for_error(e: &BridgeError) -> Status {
    let mut s = Status::default();
    match e {
        BridgeError::VigemMissing => s.vigem_ok = false,
        BridgeError::HidHideMissing => s.hidhide_ok = false,
        BridgeError::Io(_) => {}
    }
    s
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.with_bridge(|b| b.settings()).unwrap_or_default()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let before = get_settings(state.clone());
    if before.start_with_windows != settings.start_with_windows {
        autostart::set(settings.start_with_windows);
    }
    settings.save(&state.settings_path).map_err(|e| e.to_string())?;
    state.with_bridge(|b| b.update_settings(settings));
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.with_bridge(|b| b.status()).unwrap_or_else(|| status_for_error(&BridgeError::Io("bridge stopped".into())))
}

#[tauri::command]
pub fn test_rumble(state: State<'_, AppState>) {
    state.with_bridge(|b| b.test_rumble());
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    game_bar_capture: bool,
    /// Whether a Gemini app is installed (otherwise Gemini opens in the default browser).
    gemini_app: bool,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        game_bar_capture: actions::game_bar_capture_enabled(),
        gemini_app: actions::gemini_app().is_some(),
    }
}

/// What the Capture button can use right now, and what it would pick for the given choice.
#[derive(Serialize)]
pub struct CaptureInfo {
    env: actions::CaptureEnv,
    uses: actions::Recorder,
}

#[tauri::command]
pub fn capture_info(tool: pad_core::settings::CaptureTool) -> CaptureInfo {
    let env = actions::capture_env();
    // The Steam-game check looks at the window in front, which is Pad Companion while you read this:
    // treat a running Steam as "would be used in a Steam game".
    let shown = actions::CaptureEnv { steam_game: env.steam, ..env };
    CaptureInfo { env, uses: actions::choose(tool, &shown) }
}

/// Opens driver download pages and the bug-report link in the default browser (https only).
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    actions::shell_open(&url, None).then_some(()).ok_or_else(|| "could not open link".into())
}

/// Opens %LOCALAPPDATA%\PadCompanion so the user can attach log.txt to a bug report.
#[tauri::command]
pub fn open_log_folder() {
    let dir = Settings::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    actions::shell_open(&dir.to_string_lossy(), None);
}

#[tauri::command]
pub fn window_minimize(window: WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn window_maximize(window: WebviewWindow) {
    let _ = if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() };
}

/// Destroys the window (and with it the WebView2 processes); the bridge keeps running in the tray.
#[tauri::command]
pub fn window_close(window: WebviewWindow) {
    let _ = window.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_from_vigem_missing() {
        let s = status_for_error(&BridgeError::VigemMissing);
        assert!(!s.vigem_ok);
        assert!(s.connection.is_none());
    }
}
