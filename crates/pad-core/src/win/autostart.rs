//! "Start with Windows": HKCU\Software\Microsoft\Windows\CurrentVersion\Run value `PadCompanion`.

use crate::log;
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("PadCompanion");

/// Starts hidden in the tray: no window at sign-in, just the controller bridge.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" --background")
}

pub fn set(enabled: bool) {
    let r = unsafe {
        if enabled {
            let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            let data: Vec<u16> = run_command(&exe).encode_utf16().chain(Some(0)).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE)
        }
    };
    if r.is_err() && enabled {
        log!("start with Windows: registry write failed {r:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_path_and_starts_in_background() {
        assert_eq!(
            run_command(r"C:\Program Files\Pad Companion\pad-companion.exe"),
            r#""C:\Program Files\Pad Companion\pad-companion.exe" --background"#
        );
    }
}
