//! Window helpers for the Gemini overlay: which screen you're on, and showing a window over it without
//! taking focus from the app in front (so a game never pauses or leaves fullscreen).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
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
