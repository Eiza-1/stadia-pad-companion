//! The player's own Gemini API key, kept in Windows Credential Manager (protected for the signed-in
//! Windows user). Never written to settings, logs or events.

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

const TARGET: &str = "PadCompanion/GeminiApiKey";

/// Cleans a pasted key: surrounding quotes and any whitespace (a bad copy can carry line breaks).
pub fn normalize_key(raw: &str) -> String {
    raw.trim().trim_matches(|c| c == '"' || c == '\'').chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn get() -> Option<String> {
    get_in(TARGET)
}

pub fn set(key: &str) -> Result<(), String> {
    set_in(TARGET, key)
}

pub fn clear() {
    clear_in(TARGET)
}

fn get_in(target: &str) -> Option<String> {
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
        let c = &*cred;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
        CredFree(cred as *const _);
        String::from_utf8(bytes).ok()
    }
}

fn set_in(target: &str, key: &str) -> Result<(), String> {
    let mut name: Vec<u16> = target.encode_utf16().chain([0]).collect();
    let mut blob = key.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("couldn't save the key: {}", e.message()))
}

fn clear_in(target: &str) {
    let _ = unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> String {
        format!("PadCompanion/Test-{}-{}", std::process::id(), now_nanos())
    }
    fn now_nanos() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn normalize_key_trims_whitespace_and_quotes() {
        assert_eq!(normalize_key("  \"AIzaXYZ\"\n"), "AIzaXYZ");
        assert_eq!(normalize_key("'AIza 1'"), "AIza1", "inner spaces from a bad copy go too");
    }

    #[test]
    fn round_trip_and_clear() {
        let t = target();
        assert_eq!(get_in(&t), None);
        set_in(&t, "AIza1").unwrap();
        assert_eq!(get_in(&t).as_deref(), Some("AIza1"));
        set_in(&t, "AIza2").unwrap();
        assert_eq!(get_in(&t).as_deref(), Some("AIza2"), "overwrites");
        clear_in(&t);
        assert_eq!(get_in(&t), None);
    }
}
