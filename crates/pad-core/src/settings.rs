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
    /// The Gemini overlay over any app: tap to type, hold to talk.
    #[default]
    Overlay,
    /// Open the Gemini app (or gemini.google.com).
    Gemini,
    Off,
}

/// How the overlay gives Gemini's answers.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AnswerVoice {
    #[default]
    Text,
    Voice,
    Both,
}

/// How the pill appears: the Google glow that gathers into the pill, a quick fade, or no animation.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OverlayAnim {
    #[default]
    Glow,
    Fade,
    Off,
}

pub const DEFAULT_VOICE: &str = "Aoede";

/// Gemini Live: on speakers the mic closes while Gemini talks (hold Assistant to talk over it); on
/// headphones it stays open, like a call.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LiveListen {
    #[default]
    Speakers,
    Headphones,
}

/// How see-through the Live panel is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LiveGlass {
    Light,
    #[default]
    Frosted,
    Solid,
}

/// The screen dim behind the Live panel: lightens after 2 s, or stays dark.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LiveDim {
    #[default]
    EasesOff,
    StaysDark,
}

fn not_migrated() -> bool {
    false
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
    /// How game rumble is translated for this profile (rumble.rs).
    pub rumble_mode: crate::rumble::RumbleMode,
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
            rumble_mode: crate::rumble::RumbleMode::Default,
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
    pub answer_voice: AnswerVoice,
    /// Old files lack this, so it reads `false` and `normalize` moves an old `Gemini` choice to the
    /// overlay once (fresh defaults are already `Overlay`).
    #[serde(default = "not_migrated")]
    pub overlay_migrated: bool,
    /// The one-time "Assistant now opens the Gemini overlay" note was dismissed.
    pub overlay_intro_seen: bool,
    /// Where the pill was dragged to (physical px); `None` = lower centre of the screen.
    pub overlay_pos: Option<(i32, i32)>,
    /// Keyboard shortcut for the overlay (tap = type, hold = talk); `None` = off.
    pub overlay_hotkey: Option<crate::hotkey::Hotkey>,
    /// Pill size: 1.0 = Standard (600 x 64 px), kept within 0.7..=1.4.
    pub overlay_scale: f32,
    pub overlay_anim: OverlayAnim,
    /// Gemini voice that reads answers aloud (one of `gemini::VOICES`).
    pub voice: String,
    pub live_listen: LiveListen,
    pub live_glass: LiveGlass,
    pub live_dim: LiveDim,
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
            assistant: AssistantAction::Overlay,
            capture: CaptureAction::TapHold,
            capture_tool: CaptureTool::Auto,
            stadia: StadiaAction::GuideHold,
            answer_voice: AnswerVoice::Text,
            overlay_migrated: true,
            overlay_intro_seen: false,
            overlay_pos: None,
            overlay_hotkey: Some(crate::hotkey::Hotkey::default()),
            overlay_scale: 1.0,
            overlay_anim: OverlayAnim::Glow,
            voice: DEFAULT_VOICE.into(),
            live_listen: LiveListen::Speakers,
            live_glass: LiveGlass::Frosted,
            live_dim: LiveDim::EasesOff,
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

    /// Pill size within 0.7..=1.4 (Standard if the value is not a number).
    pub fn clamp_scale(v: f32) -> f32 {
        if v.is_finite() { v.clamp(0.7, 1.4) } else { 1.0 }
    }

    fn normalize(&mut self) {
        self.overlay_scale = Self::clamp_scale(self.overlay_scale);
        if !crate::gemini::VOICES.iter().any(|(v, _)| *v == self.voice) {
            self.voice = DEFAULT_VOICE.into();
        }
        if !self.overlay_migrated {
            if self.assistant == AssistantAction::Gemini {
                self.assistant = AssistantAction::Overlay;
            }
            self.overlay_migrated = true;
        }
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
    fn old_gemini_setting_moves_to_overlay_once() {
        let p = write_tmp(r#"{"assistant":"Gemini"}"#);
        let s = Settings::load(&p);
        assert_eq!(s.assistant, AssistantAction::Overlay);
        assert!(s.overlay_migrated);
        let mut back = s.clone();
        back.assistant = AssistantAction::Gemini;
        back.save(&p).unwrap();
        assert_eq!(Settings::load(&p).assistant, AssistantAction::Gemini, "a later choice of Gemini app is kept");
    }

    #[test]
    fn new_settings_default_to_overlay_and_text_answers() {
        let s = Settings::default();
        assert_eq!(s.assistant, AssistantAction::Overlay);
        assert_eq!(s.answer_voice, AnswerVoice::Text);
        assert_eq!(s.overlay_pos, None);
        assert!(!s.overlay_intro_seen);
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

    #[test]
    fn pill_defaults_to_standard_size_and_glow() {
        let s = Settings::default();
        assert_eq!(s.overlay_scale, 1.0);
        assert_eq!(s.overlay_anim, OverlayAnim::Glow);
    }

    #[test]
    fn pill_size_is_kept_in_range() {
        assert_eq!(Settings::clamp_scale(0.2), 0.7);
        assert_eq!(Settings::clamp_scale(3.0), 1.4);
        assert_eq!(Settings::clamp_scale(f32::NAN), 1.0);
        assert_eq!(Settings::clamp_scale(1.2), 1.2);
        let p = write_tmp(r#"{"overlay_scale":9.0,"overlay_anim":"Fade"}"#);
        let s = Settings::load(&p);
        assert_eq!(s.overlay_scale, 1.4);
        assert_eq!(s.overlay_anim, OverlayAnim::Fade);
    }

    #[test]
    fn read_aloud_uses_a_known_gemini_voice() {
        assert_eq!(Settings::default().voice, "Aoede");
        let p = write_tmp(r#"{"voice":"Robot9000"}"#);
        assert_eq!(Settings::load(&p).voice, "Aoede", "an unknown voice goes back to the default");
        let p = write_tmp(r#"{"voice":"Puck"}"#);
        assert_eq!(Settings::load(&p).voice, "Puck");
    }

    #[test]
    fn live_defaults_are_speakers_frosted_eases_off() {
        let s = Settings::default();
        assert_eq!(s.live_listen, LiveListen::Speakers);
        assert_eq!(s.live_glass, LiveGlass::Frosted);
        assert_eq!(s.live_dim, LiveDim::EasesOff);
        let p = write_tmp(r#"{"live_glass":"Light","live_dim":"StaysDark","live_listen":"Headphones"}"#);
        let s = Settings::load(&p);
        assert_eq!((s.live_glass, s.live_dim, s.live_listen), (LiveGlass::Light, LiveDim::StaysDark, LiveListen::Headphones));
    }
}
