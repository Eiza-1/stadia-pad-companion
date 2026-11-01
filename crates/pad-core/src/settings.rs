//! User settings and profiles, persisted as JSON in %LOCALAPPDATA%\PadCompanion\settings.json.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PadMode {
    #[default]
    Basic,
    Competitive,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PadColor {
    #[default]
    JustBlack,
    ClearlyWhite,
    Wasabi,
    NightBlue,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Curve {
    #[default]
    Linear,
    Smooth,
    Aggressive,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AssistantAction {
    #[default]
    Gemini,
    Off,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaptureAction {
    #[default]
    TapHold,
    Off,
}

/// Which recorder the Capture button drives. `Auto` picks the first one available (see actions.rs).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaptureTool {
    #[default]
    Auto,
    GameBar,
    Steam,
    Nvidia,
    Amd,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StadiaAction {
    #[default]
    GuideHold,
    GuideOnly,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum XButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    L3,
    R3,
    Back,
    Start,
    Guide,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Xbox(XButton),
    Disabled,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct StickCfg {
    pub deadzone_pct: u8,
    pub curve: Curve,
}

impl Default for StickCfg {
    fn default() -> Self {
        Self { deadzone_pct: 8, curve: Curve::Linear }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct TriggerCfg {
    pub start_pct: u8,
    pub end_pct: u8,
    pub hair: bool,
}

impl Default for TriggerCfg {
    fn default() -> Self {
        Self { start_pct: 4, end_pct: 96, hair: false }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Key = `Buttons` flag name (e.g. "A", "DPAD_UP"); missing keys use the default mapping.
    pub remap: BTreeMap<String, Target>,
    pub left: StickCfg,
    pub right: StickCfg,
    pub triggers: TriggerCfg,
    pub rumble_pct: u8,
}

pub const DEFAULT_PROFILE: &str = "Default";

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: DEFAULT_PROFILE.into(),
            remap: BTreeMap::new(),
            left: StickCfg::default(),
            right: StickCfg::default(),
            triggers: TriggerCfg::default(),
            rumble_pct: 100,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub active_profile: String,
    pub profiles: Vec<Profile>,
    pub pad_mode: PadMode,
    pub pad_color: PadColor,
    pub hide_from_games: bool,
    pub start_with_windows: bool,
    pub assistant: AssistantAction,
    pub capture: CaptureAction,
    pub capture_tool: CaptureTool,
    pub stadia: StadiaAction,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_profile: DEFAULT_PROFILE.into(),
            profiles: vec![Profile::default()],
            pad_mode: PadMode::Basic,
            pad_color: PadColor::JustBlack,
            hide_from_games: true,
            start_with_windows: false,
            assistant: AssistantAction::Gemini,
            capture: CaptureAction::TapHold,
            capture_tool: CaptureTool::Auto,
            stadia: StadiaAction::GuideHold,
        }
    }
}

impl Settings {
    /// The active profile; `load` guarantees it exists.
    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.name == self.active_profile)
            .or_else(|| self.profiles.first())
            .expect("settings always hold at least the Default profile")
    }

    /// Never fails: a missing or corrupt file gives defaults.
    pub fn load(path: &Path) -> Settings {
        let mut s: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.normalize();
        s
    }

    fn normalize(&mut self) {
        if !self.profiles.iter().any(|p| p.name == DEFAULT_PROFILE) {
            self.profiles.insert(0, Profile::default());
        }
        if !self.profiles.iter().any(|p| p.name == self.active_profile) {
            self.active_profile = DEFAULT_PROFILE.into();
        }
    }

    /// Atomic save: write `<file>.tmp`, then rename over the target.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }

    pub fn data_dir() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("PadCompanion")
    }

    pub fn default_path() -> PathBuf {
        Self::data_dir().join("settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    fn tempdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "padcompanion-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_tmp(contents: &str) -> PathBuf {
        let p = tempdir().join("settings.json");
        std::fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn roundtrip() {
        let p = tempdir().join("s.json");
        let mut s = Settings::default();
        s.pad_mode = PadMode::Competitive;
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
    }

    #[test]
    fn corrupt_or_empty_gives_defaults() {
        for c in ["", "{not json", "[]"] {
            let p = write_tmp(c);
            assert_eq!(Settings::load(&p), Settings::default());
        }
    }

    #[test]
    fn partial_file_fills_defaults() {
        let p = write_tmp(r#"{"pad_mode":"Competitive"}"#);
        let s = Settings::load(&p);
        assert_eq!(s.pad_mode, PadMode::Competitive);
        assert_eq!(s.active().name, "Default");
    }

    #[test]
    fn file_with_removed_fields_still_loads() {
        // 0.1.0 saved these; they were dropped once measurements showed they could not change anything.
        let p = write_tmp(r#"{"pad_mode":"Competitive","ble_low_latency":true,"polling_hz":250,"pad_color":"Wasabi"}"#);
        let s = Settings::load(&p);
        assert_eq!(s.pad_mode, PadMode::Competitive);
        assert_eq!(s.pad_color, PadColor::Wasabi, "the rest of the file is kept");
    }

    #[test]
    fn dangling_active_profile_falls_back() {
        let p = write_tmp(r#"{"active_profile":"Gone","profiles":[]}"#);
        assert_eq!(Settings::load(&p).active().name, "Default");
    }
}
