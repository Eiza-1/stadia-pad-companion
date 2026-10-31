//! Drives Nefarius HidHide through its CLI so games and launchers only see the virtual Xbox 360 pad.
//! Our own exe is whitelisted so it can still read the real pad. Elevates once only if a plain call isn't enough.

use crate::log;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CANDIDATES: [&str; 2] = [
    r"C:\Program Files\Nefarius Software Solutions\HidHide\x64\HidHideCLI.exe",
    r"C:\Program Files\Nefarius Software Solutions\HidHide\x86\HidHideCLI.exe",
];

fn cli() -> Option<PathBuf> {
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists())
}

pub fn installed() -> bool {
    cli().is_some()
}

fn self_path() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn query(arg: &str) -> String {
    let Some(cli) = cli() else { return String::new() };
    match Command::new(cli).arg(arg).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr),
        Err(e) => {
            log!("HidHide query failed: {e}");
            String::new()
        }
    }
}

fn hide_args(ids: &[String]) -> Vec<String> {
    let mut a = vec!["--app-reg".into(), self_path()];
    for id in ids {
        a.push("--dev-hide".into());
        a.push(id.clone());
    }
    a.push("--cloak-on".into());
    a
}

fn unhide_args(ids: &[String]) -> Vec<String> {
    ids.iter().flat_map(|id| ["--dev-unhide".to_string(), id.clone()]).collect()
}

fn run(args: &[String], elevated: bool) {
    let Some(cli) = cli() else { return };
    if !elevated {
        let r = Command::new(&cli).args(args).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output();
        log!("HidHide {args:?} -> {:?}", r.map(|o| o.status.code()));
        return;
    }
    let params = args.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    let file = HSTRING::from(cli.as_os_str());
    let params = HSTRING::from(params.as_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        match ShellExecuteExW(&mut info) {
            Ok(()) => {
                if !info.hProcess.is_invalid() {
                    WaitForSingleObject(info.hProcess, 15_000);
                    let _ = CloseHandle(info.hProcess);
                }
                log!("HidHide (admin) {args:?} done");
            }
            Err(e) => log!("HidHide elevation declined or failed: {e}"),
        }
    }
}

fn is_configured(ids: &[String]) -> bool {
    let apps = query("--app-list");
    let devs = query("--dev-list");
    let cloak = query("--cloak-state");
    contains(&apps, &self_path()) && ids.iter().all(|id| contains(&devs, id)) && cloak.contains("--cloak-on")
}

/// Whitelist this exe, hide the given devices and turn cloaking on. Touches HidHide only when something is missing.
pub fn ensure_hidden(ids: &[String]) -> bool {
    if !installed() {
        return false;
    }
    if is_configured(ids) {
        return true;
    }
    let args = hide_args(ids);
    run(&args, false);
    if is_configured(ids) {
        return true;
    }
    log!("HidHide refused unelevated change, requesting admin once");
    run(&args, true);
    is_configured(ids)
}

/// Whitelist this exe so it can still open a pad hidden earlier (by us or another tool).
/// Must run before discovery: hidden devices are invisible to non-whitelisted processes.
pub fn ensure_app_registered() -> bool {
    if !installed() {
        return false;
    }
    let me = self_path();
    if contains(&query("--app-list"), &me) {
        return true;
    }
    let args = vec!["--app-reg".to_string(), me.clone()];
    run(&args, false);
    if contains(&query("--app-list"), &me) {
        return true;
    }
    run(&args, true);
    contains(&query("--app-list"), &me)
}

/// Stadia instance ids (USB or Bluetooth LE) in `HidHideCLI --dev-list` output.
fn stadia_ids_in_dev_list(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.trim().strip_prefix("--dev-hide"))
        .map(|id| id.trim().trim_matches('"').to_string())
        .filter(|id| {
            let u = id.to_uppercase();
            (u.contains("VID_18D1") || u.contains("VID&0218D1")) && (u.contains("PID_9400") || u.contains("PID&9400"))
        })
        .collect()
}

/// Uninstall cleanup: un-hide every Stadia pad we may have hidden (connected or not) and
/// remove this exe from the HidHide whitelist.
pub fn cleanup() {
    if !installed() {
        return;
    }
    let ids = stadia_ids_in_dev_list(&query("--dev-list"));
    unhide(&ids);
    run(&["--app-unreg".into(), self_path()], false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_hidden_stadia_ids_only() {
        let out = "--dev-hide \"HID\\VID_18D1&PID_9400&MI_01\\7&249F76DE&0&0000\"\r\n--dev-hide \"HID\\VID_054C&PID_09CC\\8&1\"\r\n--dev-hide \"HID\\{00001812-0000-1000-8000-00805F9B34FB}_DEV_VID&0218D1_PID&9400_REV&0100_CA6FFE0A918E\\9&17C8AD2D&0&0000\"\r\n";
        assert_eq!(
            stadia_ids_in_dev_list(out),
            vec![
                "HID\\VID_18D1&PID_9400&MI_01\\7&249F76DE&0&0000".to_string(),
                "HID\\{00001812-0000-1000-8000-00805F9B34FB}_DEV_VID&0218D1_PID&9400_REV&0100_CA6FFE0A918E\\9&17C8AD2D&0&0000".to_string(),
            ]
        );
    }
}

/// Give the pad back to every app (on Quit or when hiding is turned off).
pub fn unhide(ids: &[String]) {
    if !installed() || ids.is_empty() {
        return;
    }
    let args = unhide_args(ids);
    run(&args, false);
    let devs = query("--dev-list");
    if ids.iter().any(|id| contains(&devs, id)) {
        run(&args, true);
    }
}
