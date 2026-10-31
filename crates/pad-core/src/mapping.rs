//! Physical Stadia state → Xbox 360 (XUSB) state: button remap, radial stick dead zone and
//! response curves, trigger ranges, and Basic-mode jitter hysteresis.

use crate::report::{Buttons, PadState};
pub use crate::settings::{Curve, PadMode, Profile, StadiaAction, StickCfg, Target, TriggerCfg, XButton};
use serde::Serialize;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct XOut {
    /// XUSB button bit layout.
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

pub fn xusb_bit(b: XButton) -> u16 {
    match b {
        XButton::DpadUp => 0x0001,
        XButton::DpadDown => 0x0002,
        XButton::DpadLeft => 0x0004,
        XButton::DpadRight => 0x0008,
        XButton::Start => 0x0010,
        XButton::Back => 0x0020,
        XButton::L3 => 0x0040,
        XButton::R3 => 0x0080,
        XButton::LB => 0x0100,
        XButton::RB => 0x0200,
        XButton::Guide => 0x0400,
        XButton::A => 0x1000,
        XButton::B => 0x2000,
        XButton::X => 0x4000,
        XButton::Y => 0x8000,
    }
}

/// Physical button → default Xbox target (`None` = not forwarded).
pub fn default_target(b: Buttons) -> Option<XButton> {
    Some(match b {
        Buttons::A => XButton::A,
        Buttons::B => XButton::B,
        Buttons::X => XButton::X,
        Buttons::Y => XButton::Y,
        Buttons::LB => XButton::LB,
        Buttons::RB => XButton::RB,
        Buttons::L3 => XButton::L3,
        Buttons::R3 => XButton::R3,
        Buttons::OPTIONS => XButton::Back,
        Buttons::MENU => XButton::Start,
        Buttons::STADIA => XButton::Guide,
        Buttons::DPAD_UP => XButton::DpadUp,
        Buttons::DPAD_DOWN => XButton::DpadDown,
        Buttons::DPAD_LEFT => XButton::DpadLeft,
        Buttons::DPAD_RIGHT => XButton::DpadRight,
        _ => return None,
    })
}

pub struct Mapper {
    /// Precomputed XUSB bit per physical flag (0 = not forwarded), indexed by bit position.
    table: [u16; 32],
    profile: Profile,
    mode: PadMode,
    pub stadia: StadiaAction,
    /// Last accepted raw axis values (lx, ly, rx, ry, l2, r2) for Basic hysteresis.
    prev: Option<[u8; 6]>,
}

impl Mapper {
    pub fn new(profile: Profile, mode: PadMode, stadia: StadiaAction) -> Self {
        let mut m = Mapper { table: [0; 32], profile, mode, stadia, prev: None };
        m.rebuild();
        m
    }

    pub fn set(&mut self, profile: Profile, mode: PadMode, stadia: StadiaAction) {
        self.profile = profile;
        self.mode = mode;
        self.stadia = stadia;
        self.prev = None;
        self.rebuild();
    }

    pub fn reset(&mut self) {
        self.prev = None;
    }

    fn rebuild(&mut self) {
        self.table = [0; 32];
        for (name, flag) in Buttons::all().iter_names() {
            let target = match self.profile.remap.get(name) {
                Some(Target::Xbox(x)) => Some(*x),
                Some(Target::Disabled) => None,
                None => default_target(flag),
            };
            self.table[flag.bits().trailing_zeros() as usize] = target.map_or(0, xusb_bit);
        }
    }

    pub fn map(&mut self, s: &PadState) -> XOut {
        let mut bits = s.buttons.bits();
        let mut buttons = 0u16;
        while bits != 0 {
            let i = bits.trailing_zeros();
            buttons |= self.table[i as usize];
            bits &= bits - 1;
        }

        let mut raw = [s.lx, s.ly, s.rx, s.ry, s.l2, s.r2];
        if self.mode == PadMode::Basic {
            if let Some(prev) = self.prev {
                for (n, p) in raw.iter_mut().zip(prev) {
                    if n.abs_diff(p) < 2 && *n != 0 && *n != 255 {
                        *n = p;
                    }
                }
            }
            self.prev = Some(raw);
        }

        let (lx, ly) = stick(raw[0], raw[1], &self.profile.left);
        let (rx, ry) = stick(raw[2], raw[3], &self.profile.right);
        XOut {
            buttons,
            lt: trigger(raw[4], &self.profile.triggers),
            rt: trigger(raw[5], &self.profile.triggers),
            lx,
            ly,
            rx,
            ry,
        }
    }
}

fn stick(x: u8, y: u8, cfg: &StickCfg) -> (i16, i16) {
    let fx = (x as f32 - 127.5) / 127.5;
    let fy = -(y as f32 - 127.5) / 127.5;
    let m = (fx * fx + fy * fy).sqrt();
    if m == 0.0 {
        return (0, 0);
    }
    let dz = cfg.deadzone_pct.min(99) as f32 / 100.0;
    let m1 = ((m - dz) / (1.0 - dz)).clamp(0.0, 1.0);
    let m2 = match cfg.curve {
        Curve::Linear => m1,
        Curve::Smooth => m1 * m1,
        Curve::Aggressive => m1.sqrt(),
    };
    let k = m2 * 32767.0 / m;
    let out = |v: f32| (v * k).round().clamp(-32768.0, 32767.0) as i16;
    (out(fx), out(fy))
}

fn trigger(raw: u8, cfg: &TriggerCfg) -> u8 {
    let s = cfg.start_pct as f32 / 100.0;
    if cfg.hair {
        return if raw as f32 > s * 255.0 { 255 } else { 0 };
    }
    let e = (cfg.end_pct as f32 / 100.0).max(s + 0.01);
    (((raw as f32 / 255.0 - s) / (e - s)).clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(b: Buttons) -> PadState {
        PadState { buttons: b, lx: 128, ly: 128, rx: 128, ry: 128, l2: 0, r2: 0 }
    }
    fn sticks(lx: u8, ly: u8) -> PadState {
        PadState { lx, ly, ..st(Buttons::empty()) }
    }
    fn trig(l2: u8) -> PadState {
        PadState { l2, ..st(Buttons::empty()) }
    }
    fn mapper(mode: PadMode) -> Mapper {
        Mapper::new(Profile::default(), mode, StadiaAction::GuideHold)
    }

    #[test]
    fn default_buttons() {
        let mut m = mapper(PadMode::Competitive);
        let o = m.map(&st(Buttons::A | Buttons::MENU | Buttons::STADIA | Buttons::CAPTURE));
        assert_eq!(o.buttons, 0x1000 | 0x0010 | 0x0400);
    }

    #[test]
    fn remap_and_disable() {
        let mut p = Profile::default();
        p.remap.insert("A".into(), Target::Xbox(XButton::B));
        p.remap.insert("Y".into(), Target::Disabled);
        let mut m = Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold);
        assert_eq!(m.map(&st(Buttons::A | Buttons::Y)).buttons, 0x2000);
    }

    #[test]
    fn centre_is_zero_and_extremes_full() {
        let mut m = mapper(PadMode::Competitive);
        let o = m.map(&sticks(128, 128));
        assert_eq!((o.lx, o.ly), (0, 0));
        let o = m.map(&sticks(255, 0));
        assert!(o.lx > 23000 && o.ly > 23000); // diagonal, normalised
        let o = m.map(&sticks(255, 128));
        assert_eq!(o.lx, 32767);
    }

    #[test]
    fn deadzone() {
        let mut m = mapper(PadMode::Competitive); // default 8%
        assert_eq!(m.map(&sticks(135, 128)).lx, 0);
        assert!(m.map(&sticks(150, 128)).lx > 0);
    }

    #[test]
    fn curves_order() {
        let v = |c| {
            let mut p = Profile::default();
            p.left.curve = c;
            p.left.deadzone_pct = 0;
            Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold).map(&sticks(192, 128)).lx
        };
        assert!(v(Curve::Aggressive) > v(Curve::Linear) && v(Curve::Linear) > v(Curve::Smooth));
    }

    #[test]
    fn triggers() {
        let mut m = mapper(PadMode::Competitive);
        assert_eq!(m.map(&trig(5)).lt, 0);
        assert_eq!(m.map(&trig(250)).lt, 255);
        let mut p = Profile::default();
        p.triggers.hair = true;
        let mut h = Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold);
        assert_eq!(h.map(&trig(20)).lt, 255);
    }

    #[test]
    fn basic_hysteresis() {
        let mut m = mapper(PadMode::Basic);
        let a = m.map(&sticks(200, 128)).lx;
        assert_eq!(m.map(&sticks(201, 128)).lx, a);
        assert_ne!(m.map(&sticks(203, 128)).lx, a);
        m.reset();
    }

    #[test]
    fn competitive_no_hysteresis() {
        let mut m = mapper(PadMode::Competitive);
        let a = m.map(&sticks(200, 128)).lx;
        assert_ne!(m.map(&sticks(201, 128)).lx, a);
    }
}
