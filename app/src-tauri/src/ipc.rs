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
pub fn set_settings(app: tauri::AppHandle, state: State<'_, AppState>, mut settings: Settings) -> Result<(), String> {
    let before = get_settings(state.clone());
    settings.overlay_scale = Settings::clamp_scale(settings.overlay_scale);
    // Owned by the overlay (drag) and its own commands, not by a window's copy of the settings.
    settings.overlay_pos = before.overlay_pos;
    settings.overlay_hotkey = before.overlay_hotkey; // changed only through set_overlay_hotkey
    if before.start_with_windows != settings.start_with_windows {
        autostart::set(settings.start_with_windows);
    }
    settings.save(&state.settings_path).map_err(|e| e.to_string())?;
    let restyle = before.overlay_scale != settings.overlay_scale || before.overlay_anim != settings.overlay_anim;
    state.with_bridge(|b| b.update_settings(settings));
    if restyle {
        crate::overlay::restyle(&app); // an open pill resizes while the slider moves
    }
    Ok(())
}

/// Changes settings from the app side (not a window's copy) and saves them.
pub fn update_settings(state: &AppState, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let Some(mut s) = state.with_bridge(|b| b.settings()) else { return Ok(()) };
    f(&mut s);
    s.save(&state.settings_path).map_err(|e| e.to_string())?;
    state.with_bridge(|b| b.update_settings(s));
    Ok(())
}

/// Settings › Gemini › Shortcut: registers it with Windows first, saves only if that worked.
#[tauri::command]
pub fn set_overlay_hotkey(
    state: State<'_, AppState>,
    keys: State<'_, pad_core::win::hotkey_win::HotkeyThread>,
    hotkey: Option<pad_core::hotkey::Hotkey>,
) -> Result<(), String> {
    keys.set(hotkey)?;
    update_settings(&state, |s| s.overlay_hotkey = hotkey)
}

/// "Ctrl + Alt + G" for the Settings row.
#[tauri::command]
pub fn hotkey_label(hotkey: pad_core::hotkey::Hotkey) -> String {
    pad_core::hotkey::label(&hotkey)
}

/// Settings › Reset overlay position.
#[tauri::command]
pub fn overlay_reset_pos(state: State<'_, AppState>) -> Result<(), String> {
    update_settings(&state, |s| s.overlay_pos = None)
}

/// Whether a Gemini key is saved (the key itself never leaves Rust).
#[tauri::command]
pub fn gemini_key_status() -> bool {
    pad_core::win::keystore::get().is_some()
}

/// Cleans, checks (one cheap request) and saves the player's key.
#[tauri::command]
pub async fn gemini_key_set(key: String) -> Result<(), String> {
    let key = pad_core::win::keystore::normalize_key(&key);
    if key.is_empty() {
        return Err("Paste your key first".into());
    }
    let k = key.clone();
    tauri::async_runtime::spawn_blocking(move || pad_core::gemini::validate_key(&k))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.message().to_string())?;
    pad_core::win::keystore::set(&key)
}

#[tauri::command]
pub fn gemini_key_clear() {
    pad_core::win::keystore::clear();
}

#[tauri::command]
pub fn gemini_open_studio() {
    actions::shell_open("https://aistudio.google.com/apikey", None);
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.with_bridge(|b| b.status()).unwrap_or_else(|| status_for_error(&BridgeError::Io("bridge stopped".into())))
}

/// The UI turns the ~60/s live pad and stats events on only while something on screen shows them (a
/// configuration page, window not minimised); Home and a minimised window get none.
#[tauri::command]
pub fn set_live(state: State<'_, AppState>, on: bool) {
    state.with_bridge(|b| b.set_live(on));
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
