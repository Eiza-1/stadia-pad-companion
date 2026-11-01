//! What the special buttons do.
//! - Assistant: the Gemini app if one is installed (Google's desktop app, a Store app or a web app
//!   installed from a browser), otherwise Gemini in the user's default browser.
//! - Capture: drives the recorder the user picked, or the first one available (Automatic): Xbox Game
//!   Bar if its captures are on, Steam while a Steam game is in front, NVIDIA or AMD if their overlay
//!   is running, otherwise a Windows screenshot (no recording).

use crate::log;
use crate::settings::CaptureTool;
use crate::special::Action;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use windows::core::{w, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_E, VK_F1, VK_F11, VK_F12, VK_F9, VK_LWIN, VK_MENU, VK_R, VK_S, VK_SHIFT, VK_SNAPSHOT,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
};

const GEMINI_URL: &str = "https://gemini.google.com/app";

// ---------- keys ----------

/// Key-down for each key in order, then key-up in reverse order.
fn chord(keys: &[VIRTUAL_KEY]) -> Vec<INPUT> {
    let key = |vk: VIRTUAL_KEY, up: bool| {
        let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
        if vk == VK_SNAPSHOT || vk == VK_LWIN {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: flags, ..Default::default() } },
        }
    };
    keys.iter().map(|&k| key(k, false)).chain(keys.iter().rev().map(|&k| key(k, true))).collect()
}

fn send_chord(keys: &[VIRTUAL_KEY]) {
    let inputs = chord(keys);
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        log!("SendInput sent {sent}/{} events", inputs.len());
    }
}

pub fn shell_open(file: &str, params: Option<&str>) -> bool {
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from);
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file.as_ptr()),
            params.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    r.0 as isize > 32
}

// ---------- Gemini ----------

fn find_gemini_window() -> Option<HWND> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let found = lp.0 as *mut Option<HWND>;
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if title.contains("Gemini") {
            *found = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut found: Option<HWND> = None;
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

/// A Start-menu shortcut for a Gemini app: Google's desktop app ("Gemini"), or a web app installed
/// from Chrome/Edge/Brave ("Gemini", sometimes "Google Gemini").
fn is_gemini_shortcut(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".lnk") else { return false };
    stem == "gemini" || stem == "google gemini"
}

fn find_shortcut(dir: &Path, depth: u32) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if e.file_name().to_str().is_some_and(is_gemini_shortcut) {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| find_shortcut(&d, depth - 1))
}

/// The installed Gemini app's Start-menu shortcut, if any (per-user first, then all users).
pub fn gemini_app() -> Option<PathBuf> {
    ["APPDATA", "ProgramData"]
        .into_iter()
        .filter_map(|v| std::env::var_os(v))
        .map(|root| PathBuf::from(root).join(r"Microsoft\Windows\Start Menu\Programs"))
        .find_map(|dir| find_shortcut(&dir, 2))
}

fn open_gemini() {
    if let Some(hwnd) = find_gemini_window() {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            // Windows only lets a background process take focus right after input; a lone Alt tap
            // (no other key) satisfies that without triggering menus in most games.
            send_chord(&[VK_MENU]);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    if let Some(app) = gemini_app() {
        if shell_open(&app.to_string_lossy(), None) {
            return;
        }
        log!("could not start the Gemini app at {}", app.display());
    }
    // ShellExecute on a URL uses the user's default browser.
    if !shell_open(GEMINI_URL, None) {
        log!("could not open Gemini");
    }
}

// ---------- Capture ----------

/// Game Bar captures are on unless the user turned them off (value missing = default on).
pub fn game_bar_capture_enabled() -> bool {
    let mut v = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\GameDVR"),
            w!("AppCaptureEnabled"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    r.is_err() || v != 0
}

/// What is available to capture with right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct CaptureEnv {
    pub game_bar: bool,
    /// Steam is running (its F12 screenshot / Ctrl+F11 recording work in games it launched).
    pub steam: bool,
    /// The window in front belongs to a game installed by Steam.
    pub steam_game: bool,
    /// NVIDIA's in-game overlay is running (its capture hotkeys need it).
    pub nvidia: bool,
    /// AMD Software: Adrenalin Edition is running.
    pub amd: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Recorder {
    GameBar,
    Steam,
    Nvidia,
    Amd,
    /// No recorder: Win+PrtScn full-screen screenshot, recording unavailable.
    Windows,
}

fn foreground_exe() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn capture_env() -> CaptureEnv {
    let procs: Vec<String> = crate::win::procs::running().into_iter().map(|n| n.to_ascii_lowercase()).collect();
    let has = |n: &str| procs.iter().any(|p| p == n);
    let steam = has("steam.exe");
    CaptureEnv {
        game_bar: game_bar_capture_enabled(),
        steam,
        steam_game: steam && foreground_exe().is_some_and(|p| p.to_ascii_lowercase().contains(r"\steamapps\")),
        nvidia: has("nvidia overlay.exe"),
        amd: has("radeonsoftware.exe"),
    }
}

/// The recorder to drive. A specific choice is used when available; otherwise (or on Auto) the first
/// available in the user's order: Game Bar, Steam (only while a Steam game is in front), NVIDIA, AMD.
pub fn choose(pref: CaptureTool, env: &CaptureEnv) -> Recorder {
    let available = |r: Recorder| match r {
        Recorder::GameBar => env.game_bar,
        Recorder::Steam => env.steam,
        Recorder::Nvidia => env.nvidia,
        Recorder::Amd => env.amd,
        Recorder::Windows => true,
    };
    let picked = match pref {
        CaptureTool::Auto => None,
        CaptureTool::GameBar => Some(Recorder::GameBar),
        CaptureTool::Steam => Some(Recorder::Steam),
        CaptureTool::Nvidia => Some(Recorder::Nvidia),
        CaptureTool::Amd => Some(Recorder::Amd),
    };
    if let Some(r) = picked.filter(|&r| available(r)) {
        return r;
    }
    if env.game_bar {
        Recorder::GameBar
    } else if env.steam_game {
        Recorder::Steam
    } else if env.nvidia {
        Recorder::Nvidia
    } else if env.amd {
        Recorder::Amd
    } else {
        Recorder::Windows
    }
}

/// Default hotkeys of each recorder (Steam: F12 / Ctrl+F11 "Record on demand"; NVIDIA app: Alt+F1 /
/// Alt+F9; AMD Adrenalin: Ctrl+Shift+S / Ctrl+Shift+E). `None`: that recorder can't do it.
pub fn keys(r: Recorder, a: Action) -> Option<&'static [VIRTUAL_KEY]> {
    match (r, a) {
        (Recorder::GameBar, Action::Screenshot) => Some(&[VK_LWIN, VK_MENU, VK_SNAPSHOT]),
        (Recorder::GameBar, Action::ToggleRecording) => Some(&[VK_LWIN, VK_MENU, VK_R]),
        (Recorder::Steam, Action::Screenshot) => Some(&[VK_F12]),
        (Recorder::Steam, Action::ToggleRecording) => Some(&[VK_CONTROL, VK_F11]),
        (Recorder::Nvidia, Action::Screenshot) => Some(&[VK_MENU, VK_F1]),
        (Recorder::Nvidia, Action::ToggleRecording) => Some(&[VK_MENU, VK_F9]),
        (Recorder::Amd, Action::Screenshot) => Some(&[VK_CONTROL, VK_SHIFT, VK_S]),
        (Recorder::Amd, Action::ToggleRecording) => Some(&[VK_CONTROL, VK_SHIFT, VK_E]),
        (Recorder::Windows, Action::Screenshot) => Some(&[VK_LWIN, VK_SNAPSHOT]),
        _ => None,
    }
}

static CAPTURE_TOOL: AtomicU8 = AtomicU8::new(0);

/// Set from the bridge whenever settings change (actions run on a worker thread without settings).
pub fn set_capture_tool(t: CaptureTool) {
    CAPTURE_TOOL.store(t as u8, Ordering::Relaxed);
}

fn capture_tool() -> CaptureTool {
    match CAPTURE_TOOL.load(Ordering::Relaxed) {
        1 => CaptureTool::GameBar,
        2 => CaptureTool::Steam,
        3 => CaptureTool::Nvidia,
        4 => CaptureTool::Amd,
        _ => CaptureTool::Auto,
    }
}

/// Runs an action. `OpenCompanion` is handled by the app (it owns the window), so it is a no-op here.
pub fn run(action: Action) {
    match action {
        Action::OpenGemini => open_gemini(),
        Action::Screenshot | Action::ToggleRecording => {
            let r = choose(capture_tool(), &capture_env());
            match keys(r, action) {
                Some(k) => send_chord(k),
                None => log!("{action:?}: {r:?} can't do this (no recorder available)"),
            }
        }
        Action::OpenCompanion => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> CaptureEnv {
        CaptureEnv::default()
    }

    #[test]
    fn automatic_follows_game_bar_then_steam_game_then_nvidia_then_amd() {
        let all = CaptureEnv { game_bar: true, steam: true, steam_game: true, nvidia: true, amd: true };
        assert_eq!(choose(CaptureTool::Auto, &all), Recorder::GameBar);
        assert_eq!(choose(CaptureTool::Auto, &CaptureEnv { game_bar: false, ..all }), Recorder::Steam);
        let no_steam_game = CaptureEnv { game_bar: false, steam_game: false, ..all };
        assert_eq!(choose(CaptureTool::Auto, &no_steam_game), Recorder::Nvidia, "Steam only drives a Steam game");
        assert_eq!(choose(CaptureTool::Auto, &CaptureEnv { nvidia: false, ..no_steam_game }), Recorder::Amd);
        assert_eq!(choose(CaptureTool::Auto, &env()), Recorder::Windows);
    }

    #[test]
    fn a_picked_recorder_is_used_when_available_else_automatic() {
        let e = CaptureEnv { game_bar: true, nvidia: true, ..env() };
        assert_eq!(choose(CaptureTool::Nvidia, &e), Recorder::Nvidia, "the user's pick beats Game Bar");
        assert_eq!(choose(CaptureTool::Amd, &e), Recorder::GameBar, "AMD missing: fall back, never do nothing");
        let steam_running = CaptureEnv { steam: true, ..env() };
        assert_eq!(choose(CaptureTool::Steam, &steam_running), Recorder::Steam, "picked Steam works without a foreground check");
    }

    #[test]
    fn each_recorder_uses_its_default_hotkeys() {
        let shot = |r| keys(r, Action::Screenshot).unwrap().to_vec();
        let rec = |r| keys(r, Action::ToggleRecording).map(|k| k.to_vec());
        assert_eq!(shot(Recorder::GameBar), vec![VK_LWIN, VK_MENU, VK_SNAPSHOT]);
        assert_eq!(shot(Recorder::Steam), vec![VK_F12]);
        assert_eq!(rec(Recorder::Steam), Some(vec![VK_CONTROL, VK_F11]));
        assert_eq!(shot(Recorder::Nvidia), vec![VK_MENU, VK_F1]);
        assert_eq!(rec(Recorder::Nvidia), Some(vec![VK_MENU, VK_F9]));
        assert_eq!(rec(Recorder::Amd), Some(vec![VK_CONTROL, VK_SHIFT, VK_E]));
        assert_eq!(shot(Recorder::Windows), vec![VK_LWIN, VK_SNAPSHOT]);
        assert_eq!(rec(Recorder::Windows), None, "no recorder: recording is unavailable, not a wrong key");
    }

    #[test]
    fn gemini_shortcut_names() {
        assert!(is_gemini_shortcut("Gemini.lnk"));
        assert!(is_gemini_shortcut("gemini.LNK"));
        assert!(is_gemini_shortcut("Google Gemini.lnk"));
        assert!(!is_gemini_shortcut("Gemini Uninstall.lnk"));
        assert!(!is_gemini_shortcut("Gemini.exe"));
    }

    #[test]
    fn chord_presses_in_order_and_releases_in_reverse() {
        let keys = [VK_LWIN, VK_MENU, VK_SNAPSHOT];
        let seq: Vec<(u16, bool)> = chord(&keys)
            .iter()
            .map(|i| unsafe { (i.Anonymous.ki.wVk.0, i.Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP)) })
            .collect();
        assert_eq!(
            seq,
            vec![(VK_LWIN.0, false), (VK_MENU.0, false), (VK_SNAPSHOT.0, false), (VK_SNAPSHOT.0, true), (VK_MENU.0, true), (VK_LWIN.0, true)]
        );
    }
}
