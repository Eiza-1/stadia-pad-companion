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

/// The Windows accent colour as `#rrggbb` (Settings > Personalisation > Colours), if set.
pub fn accent() -> Option<String> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut v = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\DWM"),
            w!("AccentColor"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    r.is_ok().then(|| accent_hex(v))
}

/// DWM stores the accent as 0xAABBGGRR.
pub fn accent_hex(abgr: u32) -> String {
    let (r, g, b) = (abgr & 0xff, (abgr >> 8) & 0xff, (abgr >> 16) & 0xff);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Keeps a window out of every screen capture (Share screen, screenshots, recordings), so Gemini
/// never sees Pad Companion's own Live panel, dim or red frame. Windows 10 2004+.
pub fn exclude_from_capture(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
    unsafe {
        let _ = SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_EXCLUDEFROMCAPTURE);
    }
}

/// Back into screen captures (after Live ends: the player's own screenshots should show the pill).
pub fn include_in_capture(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_NONE};
    unsafe {
        let _ = SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_NONE);
    }
}

/// Puts a topmost window above the other topmost ones (the Live panel over its own dim), no focus.
pub fn raise(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    unsafe {
        let _ = SetWindowPos(HWND(hwnd as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Clips a window to a rounded rectangle (client px: left, top, right, bottom, corner radius), or
/// back to its full size with `None`. Keeps the frosted-glass backdrop inside the Live panel's corners.
pub fn round_region(hwnd: isize, r: Option<(i32, i32, i32, i32, i32)>) {
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn, HRGN};
    unsafe {
        let rgn = r.map(|(l, t, rt, b, rad)| CreateRoundRectRgn(l, t, rt + 1, b + 1, rad * 2, rad * 2));
        let _ = SetWindowRgn(HWND(hwnd as *mut _), rgn.unwrap_or(HRGN::default()).into(), true);
    }
}

/// The whole screen (physical px, taskbar included) showing `hwnd`.
pub fn monitor_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mon = MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(mon, &mut mi).as_bool().then(|| {
            let r: RECT = mi.rcMonitor;
            (r.left, r.top, r.right, r.bottom)
        })
    }
}

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
    fn dwm_accent_is_read_as_rgb() {
        assert_eq!(accent_hex(0xffd77800), "#0078d7");
        assert_eq!(accent_hex(0x00112233), "#332211");
    }

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
