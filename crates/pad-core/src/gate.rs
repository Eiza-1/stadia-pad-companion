//! Hands the pad to the Gemini overlay and back. While the overlay holds the pad, the game sees a
//! released, centred pad; when it hands back, buttons still held then stay hidden from the game until
//! released, so the press that typed the last letter never reaches the game.

use crate::report::{Buttons, PadState};

/// No buttons, sticks centred, triggers up.
pub const NEUTRAL: PadState = PadState { buttons: Buttons::empty(), lx: 128, ly: 128, rx: 128, ry: 128, l2: 0, r2: 0 };

#[derive(Default)]
pub struct Gate {
    captured: bool,
    /// Buttons held when the overlay handed the pad back; hidden from the game until released.
    latched: Buttons,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    pub fn captured(&self) -> bool {
        self.captured
    }

    /// Starts or ends the overlay's hold on the pad; `current` is the pad as it is right now.
    pub fn set_captured(&mut self, on: bool, current: &PadState) {
        if self.captured && !on {
            self.latched = current.buttons;
        }
        self.captured = on;
    }

    /// What the game gets for this report.
    pub fn game_state(&self, s: &PadState) -> PadState {
        if self.captured {
            return NEUTRAL;
        }
        PadState { buttons: s.buttons - self.latched, ..*s }
    }

    /// Feeds every report: latched buttons are freed once released.
    pub fn observe(&mut self, s: &PadState) {
        self.latched &= s.buttons;
    }

    /// New session: nothing captured, nothing latched.
    pub fn reset(&mut self) {
        *self = Gate::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(buttons: Buttons) -> PadState {
        PadState { buttons, ..NEUTRAL }
    }

    #[test]
    fn captured_gives_neutral() {
        let mut g = Gate::new();
        let s = PadState { buttons: Buttons::A, lx: 255, ..NEUTRAL };
        g.set_captured(true, &s);
        assert_eq!(g.game_state(&s), NEUTRAL);
    }

    #[test]
    fn release_latches_buttons_held_at_handback() {
        let mut g = Gate::new();
        let a = st(Buttons::A);
        g.set_captured(true, &a);
        g.set_captured(false, &a);
        assert!(!g.game_state(&a).buttons.contains(Buttons::A), "A held at hand-back stays hidden");
        g.observe(&NEUTRAL);
        g.observe(&a);
        assert!(g.game_state(&a).buttons.contains(Buttons::A), "a fresh press goes through");
    }

    #[test]
    fn other_buttons_pass_while_one_is_latched() {
        let mut g = Gate::new();
        let a = st(Buttons::A);
        g.set_captured(true, &a);
        g.set_captured(false, &a);
        let ab = st(Buttons::A | Buttons::B);
        g.observe(&ab);
        assert_eq!(g.game_state(&ab).buttons, Buttons::B);
    }

    #[test]
    fn sticks_pass_through_after_handback() {
        let mut g = Gate::new();
        let s = PadState { lx: 200, ..NEUTRAL };
        g.set_captured(true, &s);
        g.set_captured(false, &s);
        assert_eq!(g.game_state(&s).lx, 200);
    }

    #[test]
    fn reset_clears_capture_and_latch() {
        let mut g = Gate::new();
        let a = st(Buttons::A);
        g.set_captured(true, &a);
        g.set_captured(false, &a);
        g.reset();
        assert!(g.game_state(&a).buttons.contains(Buttons::A));
        g.set_captured(true, &a);
        g.reset();
        assert_eq!(g.game_state(&a).buttons, Buttons::A, "reset also ends capture");
    }
}
