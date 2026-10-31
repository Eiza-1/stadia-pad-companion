//! Stadia HID input report (ID 0x03, same layout over USB and Bluetooth LE) and rumble output report.
//!
//! `[1]` hat 0..7 clockwise from up, 8 = released
//! `[2]` 0x80 R3, 0x40 Options, 0x20 Menu, 0x10 Stadia, 0x08 R2 click, 0x04 L2 click, 0x02 Assistant, 0x01 Capture
//! `[3]` 0x40 A, 0x20 B, 0x10 X, 0x08 Y, 0x04 LB, 0x02 RB, 0x01 L3
//! `[4..7]` LX LY RX RY (0..255, Y down-positive), `[8]` L2, `[9]` R2

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct Buttons: u32 {
        const A = 1 << 0;
        const B = 1 << 1;
        const X = 1 << 2;
        const Y = 1 << 3;
        const LB = 1 << 4;
        const RB = 1 << 5;
        const L3 = 1 << 6;
        const R3 = 1 << 7;
        const L2_CLICK = 1 << 8;
        const R2_CLICK = 1 << 9;
        const OPTIONS = 1 << 10;
        const MENU = 1 << 11;
        const STADIA = 1 << 12;
        const ASSISTANT = 1 << 13;
        const CAPTURE = 1 << 14;
        const DPAD_UP = 1 << 15;
        const DPAD_DOWN = 1 << 16;
        const DPAD_LEFT = 1 << 17;
        const DPAD_RIGHT = 1 << 18;
    }
}

/// Raw pad state as reported by the hardware (sticks 0..255, Y down-positive).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PadState {
    pub buttons: Buttons,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    pub l2: u8,
    pub r2: u8,
}

const HAT: [Buttons; 8] = [
    Buttons::DPAD_UP,
    Buttons::DPAD_UP.union(Buttons::DPAD_RIGHT),
    Buttons::DPAD_RIGHT,
    Buttons::DPAD_DOWN.union(Buttons::DPAD_RIGHT),
    Buttons::DPAD_DOWN,
    Buttons::DPAD_DOWN.union(Buttons::DPAD_LEFT),
    Buttons::DPAD_LEFT,
    Buttons::DPAD_UP.union(Buttons::DPAD_LEFT),
];

const BYTE2: [(u8, Buttons); 8] = [
    (0x80, Buttons::R3),
    (0x40, Buttons::OPTIONS),
    (0x20, Buttons::MENU),
    (0x10, Buttons::STADIA),
    (0x08, Buttons::R2_CLICK),
    (0x04, Buttons::L2_CLICK),
    (0x02, Buttons::ASSISTANT),
    (0x01, Buttons::CAPTURE),
];

const BYTE3: [(u8, Buttons); 7] = [
    (0x40, Buttons::A),
    (0x20, Buttons::B),
    (0x10, Buttons::X),
    (0x08, Buttons::Y),
    (0x04, Buttons::LB),
    (0x02, Buttons::RB),
    (0x01, Buttons::L3),
];

pub fn parse_input(r: &[u8]) -> Option<PadState> {
    if r.len() < 10 || r[0] != 0x03 {
        return None;
    }
    let mut buttons = HAT.get(r[1] as usize).copied().unwrap_or_default();
    for (mask, b) in BYTE2 {
        if r[2] & mask != 0 {
            buttons |= b;
        }
    }
    for (mask, b) in BYTE3 {
        if r[3] & mask != 0 {
            buttons |= b;
        }
    }
    Some(PadState { buttons, lx: r[4], ly: r[5], rx: r[6], ry: r[7], l2: r[8], r2: r[9] })
}

/// Output report 0x05: large motor LE16, small motor LE16, zero-padded to the device's report length.
pub fn rumble_report(large: u8, small: u8, len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len.max(5)];
    v[0] = 0x05;
    v[1..3].copy_from_slice(&(large as u16 * 257).to_le_bytes());
    v[3..5].copy_from_slice(&(small as u16 * 257).to_le_bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_or_wrong_id() {
        assert!(parse_input(&[3; 9]).is_none());
        assert!(parse_input(&[1, 8, 0, 0, 128, 128, 128, 128, 0, 0]).is_none());
    }

    #[test]
    fn neutral() {
        let s = parse_input(&[3, 8, 0, 0, 128, 128, 128, 128, 0, 0]).unwrap();
        assert_eq!(s.buttons, Buttons::empty());
        assert_eq!((s.lx, s.ly, s.l2), (128, 128, 0));
    }

    #[test]
    fn hat_diagonals() {
        let ur = parse_input(&[3, 1, 0, 0, 128, 128, 128, 128, 0, 0]).unwrap();
        assert_eq!(ur.buttons, Buttons::DPAD_UP | Buttons::DPAD_RIGHT);
        let l = parse_input(&[3, 6, 0, 0, 128, 128, 128, 128, 0, 0]).unwrap();
        assert_eq!(l.buttons, Buttons::DPAD_LEFT);
    }

    #[test]
    fn bits() {
        let s = parse_input(&[3, 8, 0x13, 0x41, 0, 255, 10, 20, 30, 40]).unwrap();
        assert_eq!(s.buttons, Buttons::STADIA | Buttons::ASSISTANT | Buttons::CAPTURE | Buttons::A | Buttons::L3);
        assert_eq!((s.lx, s.ly, s.rx, s.ry, s.l2, s.r2), (0, 255, 10, 20, 30, 40));
    }

    #[test]
    fn rumble() {
        assert_eq!(rumble_report(255, 128, 5), vec![5, 0xFF, 0xFF, 0x80, 0x80]);
        assert_eq!(rumble_report(0, 0, 8).len(), 8);
    }
}
