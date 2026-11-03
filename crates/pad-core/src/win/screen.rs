//! Share screen: a scaled JPEG of the screen the game is on, about once a second while sharing.
//! Pad Companion's own Live windows are hidden from capture (`overlay_win::exclude_from_capture`), so
//! Gemini sees only the game.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW,
    MonitorFromWindow, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HALFTONE, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, SRCCOPY,
};

const JPEG_QUALITY: u8 = 70;

/// Size that fits within `long_side` on the longer edge, keeping the aspect (never enlarged).
pub fn fit(w: u32, h: u32, long_side: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= long_side || long == 0 {
        return (w, h);
    }
    ((w as u64 * long_side as u64 / long as u64) as u32, (h as u64 * long_side as u64 / long as u64) as u32)
}

/// A JPEG of the monitor showing `monitor_of` (0 = the primary screen), scaled to `long_side`.
pub fn capture_jpeg(monitor_of: isize, long_side: u32) -> Option<Vec<u8>> {
    unsafe {
        let mon = MonitorFromWindow(HWND(monitor_of as *mut _), MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return None;
        }
        let r = mi.rcMonitor;
        let (sw, sh) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        let (w, h) = fit(sw, sh, long_side);
        if w == 0 || h == 0 {
            return None;
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
        let old = SelectObject(mem, bmp.into());
        SetStretchBltMode(mem, HALFTONE);
        let ok = StretchBlt(mem, 0, 0, w as i32, h as i32, Some(screen), r.left, r.top, sw as i32, sh as i32, SRCCOPY | CAPTUREBLT).as_bool();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let rows = if ok { GetDIBits(mem, bmp, 0, h, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS) } else { 0 };
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if rows == 0 {
            return None;
        }
        let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY).encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).ok()?;
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit(1920, 1080, 1024), (1024, 576));
        assert_eq!(fit(1080, 1920, 1024), (576, 1024));
        assert_eq!(fit(800, 600, 1024), (800, 600));
        assert_eq!(fit(0, 0, 1024), (0, 0));
    }

    /// Grabs the real desktop (in memory only), so it runs only when asked: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn capture_returns_a_jpeg() {
        let j = capture_jpeg(0, 1024).expect("a frame of the desktop");
        assert_eq!(&j[..2], &[0xFF, 0xD8]);
        assert!(j.len() > 1000);
    }
}
