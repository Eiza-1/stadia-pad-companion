//! The overlay's keyboard shortcut: which keys, whether a combination is allowed, how it reads, and tap
//! vs hold timing (the same as the Assistant button).

use crate::special::{Action, TapHold};
use serde::{Deserialize, Serialize};

pub const VK_SPACE: u16 = 0x20;
pub const VK_PAUSE: u16 = 0x13;
pub const VK_F13: u16 = 0x7C;
const VK_TAB: u16 = 0x09;
const VK_ENTER: u16 = 0x0D;
const VK_BACK: u16 = 0x08;

/// A key plus modifiers, as Windows virtual-key code `vk`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u16,
}

impl Default for Hotkey {
    /// Ctrl + Alt + G (G for Gemini).
    fn default() -> Self {
        Hotkey { ctrl: true, alt: true, shift: false, win: false, vk: 0x47 }
    }
}

/// Refuses keys that would stop normal typing everywhere: letters, digits, Space, Enter, Tab and
/// Backspace need Ctrl, Alt or Win (Shift alone still types them).
pub fn validate(h: &Hotkey) -> Result<(), &'static str> {
    let typing = matches!(h.vk, 0x30..=0x39 | 0x41..=0x5A | VK_SPACE | VK_ENTER | VK_TAB | VK_BACK);
    if typing && !(h.ctrl || h.alt || h.win) {
        return Err("Add Ctrl, Alt or Win to that key, or it would stop typing it anywhere");
    }
    Ok(())
}

fn key_name(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        VK_SPACE => "Space".into(),
        VK_PAUSE => "Pause".into(),
        VK_ENTER => "Enter".into(),
        VK_TAB => "Tab".into(),
        VK_BACK => "Backspace".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "Page Up".into(),
        0x22 => "Page Down".into(),
        0x91 => "Scroll Lock".into(),
        0x2C => "Print Screen".into(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0xC0 => "`".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => format!("Key {vk:#04X}"),
    }
}

/// "Ctrl + Alt + G", with modifiers in Windows' order (Win, Ctrl, Alt, Shift).
pub fn label(h: &Hotkey) -> String {
    let mut parts: Vec<String> = [(h.win, "Win"), (h.ctrl, "Ctrl"), (h.alt, "Alt"), (h.shift, "Shift")]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| n.to_string())
        .collect();
    parts.push(key_name(h.vk));
    parts.join(" + ")
}

/// Tap vs hold for the shortcut: Windows reports only the press, so the caller polls the key while it
/// is held and feeds `still_down`.
#[derive(Default)]
pub struct KeyHold {
    tap: TapHold,
    active: bool,
}

impl KeyHold {
    pub fn pressed(&mut self, now_ms: u64) {
        self.active = true;
        self.tap.clear();
        self.tap.update(true, now_ms);
    }

    pub fn poll(&mut self, now_ms: u64, still_down: bool) -> Option<Action> {
        if !self.active {
            return None;
        }
        if !still_down {
            self.active = false;
        }
        self.tap.update(still_down, now_ms)
    }

    /// Still waiting for the release.
    pub fn active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::special::Action;

    const G: u16 = 0x47;

    fn k(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u16) -> Hotkey {
        Hotkey { ctrl, alt, shift, win, vk }
    }

    #[test]
    fn plain_typing_keys_need_a_modifier() {
        assert!(validate(&k(false, false, false, false, G)).is_err());
        assert!(validate(&k(false, false, false, false, VK_SPACE)).is_err());
        assert!(validate(&k(false, false, true, false, G)).is_err(), "Shift + G still types G");
        assert!(validate(&k(false, false, false, false, 0x35)).is_err(), "digit 5");
        assert!(validate(&k(true, true, false, false, G)).is_ok());
        assert!(validate(&k(false, false, false, false, VK_F13)).is_ok());
        assert!(validate(&k(false, false, false, false, VK_PAUSE)).is_ok());
        assert!(validate(&k(false, false, false, true, G)).is_ok());
    }

    #[test]
    fn labels_read_like_windows() {
        assert_eq!(label(&k(true, true, false, false, G)), "Ctrl + Alt + G");
        assert_eq!(label(&k(false, false, true, true, VK_F13)), "Win + Shift + F13");
        assert_eq!(label(&k(false, false, false, false, VK_PAUSE)), "Pause");
        assert_eq!(label(&k(true, false, false, false, VK_SPACE)), "Ctrl + Space");
    }

    #[test]
    fn default_is_ctrl_alt_g() {
        assert_eq!(Hotkey::default(), k(true, true, false, false, G));
        assert_eq!(crate::settings::Settings::default().overlay_hotkey, Some(Hotkey::default()));
    }

    #[test]
    fn key_hold_taps_and_talks_like_the_button() {
        let mut h = KeyHold::default();
        h.pressed(0);
        assert_eq!(h.poll(120, false), Some(Action::OverlayTap));
        h.pressed(1000);
        assert_eq!(h.poll(1200, true), None);
        assert_eq!(h.poll(1300, true), Some(Action::TalkStart));
        assert_eq!(h.poll(1800, false), Some(Action::TalkEnd));
        assert_eq!(h.poll(1900, false), None);
        assert!(!h.active());
    }

    #[test]
    fn hotkey_setting_round_trips_including_off() {
        let mut s = crate::settings::Settings::default();
        s.overlay_hotkey = None;
        let back: crate::settings::Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.overlay_hotkey, None);
        let old: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.overlay_hotkey, Some(Hotkey::default()), "old files get the default");
    }
}
