//! Window helpers for the Gemini overlay: which screen you're on, and showing a window over it without
//! taking focus from the app in front (so a game never pauses or leaves fullscreen).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

/// The window in front right now (as a plain number, so it can be kept and sent between threads).
pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// Work area (left, top, right, bottom; physical px, taskbar excluded) of the screen showing `hwnd`.
pub fn work_area(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mon = MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(mon, &mut mi).as_bool().then(|| {
            let RECT { left, top, right, bottom } = mi.rcWork;
            (left, top, right, bottom)
        })
    }
}

/// `true`: clicks and showing never activate the window, and it stays off Alt+Tab. `false`: it can
/// take focus (the player clicked into the text field to type on a real keyboard).
pub fn set_no_activate(hwnd: isize, on: bool) {
    unsafe {
        let h = HWND(hwnd as *mut _);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let bits = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        let ex = if on { ex | bits } else { (ex & !(WS_EX_NOACTIVATE.0 as isize)) | WS_EX_TOOLWINDOW.0 as isize };
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
    }
}

/// Brings a window to the front with focus (the overlay field, or back to the game afterwards).
pub fn focus(hwnd: isize) {
    if hwnd != 0 {
        unsafe {
            let _ = SetForegroundWindow(HWND(hwnd as *mut _));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    #[test]
    fn a_spot_on_an_unplugged_screen_is_not_on_screen() {
        assert!(!on_screen(-100_000, -100_000));
        let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        assert!(on_screen(w / 2, h / 2), "the primary screen's centre is");
    }
}

/// Whether a point (physical px) is on a screen that exists now (a saved spot may be on an unplugged one).
pub fn on_screen(x: i32, y: i32) -> bool {
    unsafe { !MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL).is_invalid() }
}
