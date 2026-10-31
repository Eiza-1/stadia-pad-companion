//! What the special buttons do: Assistant → Gemini, Capture → Game Bar screenshot / recording.

use crate::log;
use crate::special::Action;
use windows::core::{w, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_LWIN, VK_MENU, VK_R, VK_SNAPSHOT,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
    SW_SHOWNORMAL,
};

const GEMINI_URL: &str = "https://gemini.google.com/app";

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
    let args = format!("--app={GEMINI_URL} --window-size=480,720");
    if !shell_open("msedge.exe", Some(&args)) && !shell_open(GEMINI_URL, None) {
        log!("could not open Gemini");
    }
}

/// Game Bar screenshot (Videos\Captures, captures the game window) when Game Bar capture is on;
/// otherwise Windows' own full-screen screenshot (Pictures\Screenshots), which needs no setup.
fn screenshot_keys(game_bar: bool) -> &'static [VIRTUAL_KEY] {
    if game_bar {
        &[VK_LWIN, VK_MENU, VK_SNAPSHOT]
    } else {
        &[VK_LWIN, VK_SNAPSHOT]
    }
}

/// Runs an action. `OpenCompanion` is handled by the app (it owns the window), so it is a no-op here.
pub fn run(action: Action) {
    match action {
        Action::OpenGemini => open_gemini(),
        Action::Screenshot => send_chord(screenshot_keys(game_bar_capture_enabled())),
        Action::ToggleRecording => {
            if !game_bar_capture_enabled() {
                log!("recording requested but Game Bar capture is off");
            }
            send_chord(&[VK_LWIN, VK_MENU, VK_R])
        }
        Action::OpenCompanion => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_falls_back_to_win_prtscn_without_game_bar() {
        assert_eq!(screenshot_keys(true), &[VK_LWIN, VK_MENU, VK_SNAPSHOT]);
        assert_eq!(screenshot_keys(false), &[VK_LWIN, VK_SNAPSHOT]);
    }

    #[test]
    fn chord_presses_in_order_and_releases_in_reverse() {
        let keys = [VK_LWIN, VK_MENU, VK_SNAPSHOT];
        let seq: Vec<(u16, bool)> = chord(&keys).iter().map(|i| unsafe { (i.Anonymous.ki.wVk.0, i.Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP)) }).collect();
        assert_eq!(
            seq,
            vec![(VK_LWIN.0, false), (VK_MENU.0, false), (VK_SNAPSHOT.0, false), (VK_SNAPSHOT.0, true), (VK_MENU.0, true), (VK_LWIN.0, true)]
        );
    }
}
