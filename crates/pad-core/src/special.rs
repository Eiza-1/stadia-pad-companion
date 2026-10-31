//! Assistant / Capture / Stadia button actions (tap and hold), driven by report timestamps.

use crate::report::{Buttons, PadState};
use crate::settings::{AssistantAction, CaptureAction, StadiaAction};

pub const CAPTURE_HOLD_MS: u64 = 500;
pub const STADIA_HOLD_MS: u64 = 1000;

#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
pub enum Action {
    OpenGemini,
    Screenshot,
    ToggleRecording,
    OpenCompanion,
}

/// A held button: when it went down, and whether its hold action already fired.
#[derive(Clone, Copy)]
struct Hold {
    since: u64,
    fired: bool,
}

pub struct Special {
    assistant: AssistantAction,
    capture: CaptureAction,
    stadia: StadiaAction,
    prev: Buttons,
    capture_hold: Option<Hold>,
    stadia_hold: Option<Hold>,
}

impl Special {
    pub fn new(a: AssistantAction, c: CaptureAction, s: StadiaAction) -> Self {
        Special { assistant: a, capture: c, stadia: s, prev: Buttons::empty(), capture_hold: None, stadia_hold: None }
    }

    pub fn set(&mut self, a: AssistantAction, c: CaptureAction, s: StadiaAction) {
        self.assistant = a;
        self.capture = c;
        self.stadia = s;
        self.capture_hold = None;
        self.stadia_hold = None;
    }

    pub fn update(&mut self, s: &PadState, now_ms: u64) -> Vec<Action> {
        let mut out = Vec::new();
        let down = s.buttons;
        let pressed = down - self.prev;
        self.prev = down;

        if self.assistant == AssistantAction::Gemini && pressed.contains(Buttons::ASSISTANT) {
            out.push(Action::OpenGemini);
        }

        if self.capture == CaptureAction::TapHold {
            match (down.contains(Buttons::CAPTURE), &mut self.capture_hold) {
                (true, None) => self.capture_hold = Some(Hold { since: now_ms, fired: false }),
                (true, Some(h)) => {
                    if !h.fired && now_ms.saturating_sub(h.since) >= CAPTURE_HOLD_MS {
                        h.fired = true;
                        out.push(Action::ToggleRecording);
                    }
                }
                (false, Some(h)) => {
                    if !h.fired {
                        out.push(if now_ms.saturating_sub(h.since) >= CAPTURE_HOLD_MS {
                            Action::ToggleRecording
                        } else {
                            Action::Screenshot
                        });
                    }
                    self.capture_hold = None;
                }
                (false, None) => {}
            }
        }

        if self.stadia == StadiaAction::GuideHold {
            match (down.contains(Buttons::STADIA), &mut self.stadia_hold) {
                (true, None) => self.stadia_hold = Some(Hold { since: now_ms, fired: false }),
                (true, Some(h)) => {
                    if !h.fired && now_ms.saturating_sub(h.since) >= STADIA_HOLD_MS {
                        h.fired = true;
                        out.push(Action::OpenCompanion);
                    }
                }
                (false, _) => self.stadia_hold = None,
            }
        }

        out
    }

    /// Earliest time a held button crosses its hold threshold, so the caller can wake without a new report.
    pub fn next_deadline(&self) -> Option<u64> {
        let c = self.capture_hold.filter(|h| !h.fired).map(|h| h.since + CAPTURE_HOLD_MS);
        let s = self.stadia_hold.filter(|h| !h.fired).map(|h| h.since + STADIA_HOLD_MS);
        match (c, s) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(buttons: Buttons) -> PadState {
        PadState { buttons, ..PadState::default() }
    }
    fn sp() -> Special {
        Special::new(AssistantAction::Gemini, CaptureAction::TapHold, StadiaAction::GuideHold)
    }

    #[test]
    fn assistant_edge_once() {
        let mut s = sp();
        assert_eq!(s.update(&b(Buttons::ASSISTANT), 0), vec![Action::OpenGemini]);
        assert!(s.update(&b(Buttons::ASSISTANT), 10).is_empty());
    }

    #[test]
    fn capture_tap() {
        let mut s = sp();
        s.update(&b(Buttons::CAPTURE), 0);
        assert_eq!(s.update(&b(Buttons::empty()), 200), vec![Action::Screenshot]);
    }

    #[test]
    fn capture_hold() {
        let mut s = sp();
        s.update(&b(Buttons::CAPTURE), 0);
        assert_eq!(s.next_deadline(), Some(500));
        assert_eq!(s.update(&b(Buttons::CAPTURE), 500), vec![Action::ToggleRecording]);
        assert!(s.update(&b(Buttons::empty()), 900).is_empty());
    }

    #[test]
    fn stadia_hold() {
        let mut s = sp();
        s.update(&b(Buttons::STADIA), 0);
        assert!(s.update(&b(Buttons::STADIA), 999).is_empty());
        assert_eq!(s.update(&b(Buttons::STADIA), 1000), vec![Action::OpenCompanion]);
    }

    #[test]
    fn off_does_nothing() {
        let mut s = Special::new(AssistantAction::Off, CaptureAction::Off, StadiaAction::GuideOnly);
        assert!(s.update(&b(Buttons::ASSISTANT | Buttons::CAPTURE | Buttons::STADIA), 0).is_empty());
        assert!(s.update(&b(Buttons::empty()), 2000).is_empty());
    }
}
