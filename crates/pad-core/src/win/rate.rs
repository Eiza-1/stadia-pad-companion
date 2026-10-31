//! USB polling above the pad's native 125 Hz needs an interrupt-interval override filter driver
//! (hidusbf). We don't ship one; higher rates are offered only when it is installed.

use windows::core::w;
use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ};

pub fn override_driver_present() -> bool {
    let mut key = HKEY::default();
    let ok = unsafe {
        RegOpenKeyExW(HKEY_LOCAL_MACHINE, w!(r"SYSTEM\CurrentControlSet\Services\hidusbf"), None, KEY_READ, &mut key)
    }
    .is_ok();
    if ok {
        unsafe {
            let _ = RegCloseKey(key);
        }
    }
    ok
}
