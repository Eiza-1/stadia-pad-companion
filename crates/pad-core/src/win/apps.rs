//! Runs PC commands from the overlay: opens installed apps (the Start menu's list, desktop and Store
//! apps alike) and presses the media / volume keys.

use crate::log;
use crate::pccmd::{best_app, App, PcCommand};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Each volume key press is 2 %; one command moves 10 %.
const VOLUME_STEPS: usize = 5;

static APPS: Mutex<Vec<App>> = Mutex::new(Vec::new());

/// `Get-StartApps | ConvertTo-Json` output (one app comes back as an object, several as an array).
pub fn parse_start_apps(json: &str) -> Vec<App> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap_or_default();
    let items = match v {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => vec![],
    };
    items
        .iter()
        .filter_map(|i| Some(App { name: i["Name"].as_str()?.to_string(), id: i["AppID"].as_str()?.to_string() }))
        .filter(|a| !a.name.is_empty() && !a.id.is_empty())
        .collect()
}

fn load_apps() -> Vec<App> {
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) => parse_start_apps(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            log!("listing apps failed: {e}");
            vec![]
        }
    }
}

/// Reads the app list in the background (the overlay calls this as it opens, so "open …" is instant).
pub fn warm() {
    let _ = std::thread::Builder::new().name("apps-list".into()).spawn(|| {
        let mut apps = APPS.lock().unwrap_or_else(|e| e.into_inner());
        if apps.is_empty() {
            *apps = load_apps();
        }
    });
}

/// The best installed app for `name` (the list is read once, and again when nothing matches, in case
/// something was installed since).
fn find(name: &str) -> Option<App> {
    let mut apps = APPS.lock().unwrap_or_else(|e| e.into_inner());
    if apps.is_empty() {
        *apps = load_apps();
    }
    if let Some(a) = best_app(name, &apps) {
        return Some(a.clone());
    }
    *apps = load_apps();
    best_app(name, &apps).cloned()
}

/// Runs `cmd`; returns the one-line confirmation for the answer card, or why it couldn't.
pub fn run(cmd: &PcCommand) -> Result<String, String> {
    use PcCommand::*;
    use super::actions::send_chord;
    match cmd {
        Open(name) => {
            let app = find(name).ok_or_else(|| format!("I couldn't find an app called \"{name}\" on this PC"))?;
            Command::new("explorer.exe")
                .arg(format!(r"shell:AppsFolder\{}", app.id))
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Couldn't open {}: {e}", app.name))?;
            Ok(format!("Opening {}", app.name))
        }
        PlayPause => {
            send_chord(&[VK_MEDIA_PLAY_PAUSE]);
            Ok("Play / pause".into())
        }
        NextTrack => {
            send_chord(&[VK_MEDIA_NEXT_TRACK]);
            Ok("Next track".into())
        }
        PreviousTrack => {
            send_chord(&[VK_MEDIA_PREV_TRACK]);
            Ok("Previous track".into())
        }
        VolumeUp | VolumeDown => {
            let k = if *cmd == VolumeUp { VK_VOLUME_UP } else { VK_VOLUME_DOWN };
            for _ in 0..VOLUME_STEPS {
                send_chord(&[k]);
            }
            Ok(if *cmd == VolumeUp { "Volume up".into() } else { "Volume down".into() })
        }
        Mute => {
            send_chord(&[VK_VOLUME_MUTE]);
            Ok("Sound muted / unmuted".into())
        }
        Screenshot => {
            super::actions::run(crate::special::Action::Screenshot);
            Ok("Screenshot taken".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_apps_json_reads_one_or_many() {
        let many = r#"[{"Name":"Spotify","AppID":"SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"},{"Name":"File Explorer","AppID":"Microsoft.Windows.Explorer"}]"#;
        let apps = parse_start_apps(many);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Spotify");
        assert_eq!(apps[1].id, "Microsoft.Windows.Explorer");
        assert_eq!(parse_start_apps(r#"{"Name":"Steam","AppID":"C:\\Steam\\steam.exe"}"#).len(), 1);
        assert!(parse_start_apps("").is_empty());
    }

    #[test]
    fn this_pc_lists_file_explorer() {
        // Every Windows install has it; proves the Start menu list is read.
        let apps = load_apps();
        assert!(best_app("file explorer", &apps).is_some(), "{} apps listed", apps.len());
    }
}
