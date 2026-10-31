//! Tiny append-only log in %LOCALAPPDATA%\PadCompanion\log.txt, rotated at 1 MB.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1 << 20;
static LOCK: Mutex<()> = Mutex::new(());

pub fn write(msg: &str) {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = crate::settings::Settings::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("log.txt");
    if std::fs::metadata(&path).map_or(false, |m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("log.old.txt"));
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{secs} {msg}");
    }
    #[cfg(debug_assertions)]
    eprintln!("{msg}");
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::win::log::write(&format!($($t)*)) };
}
