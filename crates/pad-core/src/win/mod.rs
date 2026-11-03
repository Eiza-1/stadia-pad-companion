//! Windows-only glue: HID access, HidHide, ViGEm, logging.

pub mod hid;
pub mod hotkey_win;
pub mod keystore;
pub mod hidhide;
pub mod log;
pub mod perf;
pub mod procs;
pub mod actions;
pub mod autostart;
pub mod ble;
pub mod vigem;
pub mod overlay_win;
pub mod speech;
pub mod sound;
pub mod apps;
pub mod audio;
pub mod screen;

/// Joins the COM multithreaded apartment on the calling thread (WinRT calls and blocking `.get()`).
pub fn com_mta() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}
