//! Running processes: other controller mappers that would fight us for the pad (double input), and
//! the capture tools the Capture button can drive.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

const MAPPERS: [&str; 5] = ["stadiapad.exe", "stadiabridge.exe", "stadiem.exe", "ds4windows.exe", "x360ce.exe"];

/// Executable names of every running process (as reported, e.g. `Steam.exe`).
pub fn running() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return names };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            names.push(String::from_utf16_lossy(&e.szExeFile[..len]));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    names
}

pub fn other_mappers() -> Vec<String> {
    let mut found = Vec::new();
    for name in running() {
        if MAPPERS.contains(&name.to_lowercase().as_str()) && !found.contains(&name) {
            found.push(name);
        }
    }
    found
}
