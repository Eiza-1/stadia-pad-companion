//! Assistant / Capture / Stadia button actions (tap and hold), driven by report timestamps.

use crate::report::{Buttons, PadState};
use crate::settings::{AssistantAction, CaptureAction, StadiaAction};

pub const CAPTURE_HOLD_MS: u64 = 500;
pub const STADIA_HOLD_MS: u64 = 1000;
/// Assistant (and the overlay keyboard shortcut): held this long = talk, shorter = tap.
pub const ASSIST_HOLD_MS: u64 = 300;
/// A second press this soon after a tap's release is a double-tap (Gemini Live); a lone tap is reported
/// once this has passed.
pub const DOUBLE_TAP_MS: u64 = 300;

#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
pub enum Action {
    OpenGemini,
    Screenshot,
    ToggleRecording,
    OpenCompanion,
    /// Overlay: Assistant tapped (open to type, or close).
    OverlayTap,
    /// Overlay: Assistant held past ASSIST_HOLD_MS: start listening.
    TalkStart,
    /// Overlay: Assistant released after TalkStart: stop listening and send.
    TalkEnd,
    /// Overlay: Assistant double-tapped: start Gemini Live.
    LiveStart,
}

/// Tap, double-tap and hold-to-talk for one button or key, driven by timestamps (ms). A tap is
/// reported DOUBLE_TAP_MS after its release unless a second press comes first (LiveStart, at once).
#[derive(Default, Clone, Copy)]
pub struct TapHold {
    hold: Option<Hold>,
    /// A tap's release time, while waiting to see if a second press follows.
    tap_at: Option<u64>,
    /// The second press of a double-tap is still down: its release means nothing.
    swallow: bool,
}

impl TapHold {
    /// Feeds the button's current state; returns OverlayTap, TalkStart, TalkEnd or LiveStart.
    pub fn update(&mut self, down: bool, now_ms: u64) -> Option<Action> {
        let tap_due = self.tap_at.is_some_and(|t| now_ms >= t + DOUBLE_TAP_MS);
        match (down, &mut self.hold) {
            (true, None) if self.swallow => None,
            (true, None) => {
                if self.tap_at.take().is_some() && !tap_due {
                    self.swallow = true;
                    return Some(Action::LiveStart);
                }
                self.hold = Some(Hold { since: now_ms, fired: false });
                tap_due.then_some(Action::OverlayTap) // a late report: the old tap still counts
            }
            (true, Some(h)) => (!h.fired && now_ms.saturating_sub(h.since) >= ASSIST_HOLD_MS).then(|| {
                h.fired = true;
                Action::TalkStart
            }),
            (false, Some(h)) => {
                let fired = h.fired;
                self.hold = None;
                if fired {
                    Some(Action::TalkEnd)
                } else {
                    self.tap_at = Some(now_ms);
                    None
                }
            }
            (false, None) if self.swallow => {
                self.swallow = false;
                None
            }
            (false, None) if tap_due => {
                self.tap_at = None;
                Some(Action::OverlayTap)
            }
            (false, None) => None,
        }
    }

    /// When something is next due: a pending press becoming a hold, or a waiting tap being reported.
    pub fn deadline(&self) -> Option<u64> {
        let hold = self.hold.filter(|h| !h.fired).map(|h| h.since + ASSIST_HOLD_MS);
        let tap = self.tap_at.map(|t| t + DOUBLE_TAP_MS);
        hold.into_iter().chain(tap).min()
    }

    /// A press, a hold or a waiting tap is in progress.
    pub fn busy(&self) -> bool {
        self.hold.is_some() || self.tap_at.is_some() || self.swallow
    }

    pub fn clear(&mut self) {
        *self = TapHold::default();
    }
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
    assist: TapHold,
}

impl Special {
    pub fn new(a: AssistantAction, c: CaptureAction, s: StadiaAction) -> Self {
        Special { assistant: a, capture: c, stadia: s, prev: Buttons::empty(), capture_hold: None, stadia_hold: None, assist: TapHold::default() }
    }

    pub fn set(&mut self, a: AssistantAction, c: CaptureAction, s: StadiaAction) {
        // A save that leaves the Assistant choice alone must not cut a talk in progress.
        if a != self.assistant {
            self.assist.clear();
        }
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
        if self.assistant == AssistantAction::Overlay {
            out.extend(self.assist.update(down.contains(Buttons::ASSISTANT), now_ms));
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
        [c, s, self.assist.deadline()].into_iter().flatten().min()
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

    fn ov() -> Special {
        Special::new(AssistantAction::Overlay, CaptureAction::TapHold, StadiaAction::GuideHold)
    }

    #[test]
    fn overlay_tap_fires_on_quick_release() {
        let mut s = ov();
        assert!(s.update(&b(Buttons::ASSISTANT), 0).is_empty(), "nothing on press");
        assert!(s.update(&b(Buttons::empty()), 120).is_empty(), "waits for a possible second tap");
        assert_eq!(s.update(&b(Buttons::empty()), 120 + DOUBLE_TAP_MS), vec![Action::OverlayTap]);
    }

    #[test]
    fn overlay_hold_starts_and_ends_talk() {
        let mut s = ov();
        s.update(&b(Buttons::ASSISTANT), 0);
        assert_eq!(s.next_deadline(), Some(ASSIST_HOLD_MS));
        assert!(s.update(&b(Buttons::ASSISTANT), 299).is_empty());
        assert_eq!(s.update(&b(Buttons::ASSISTANT), 300), vec![Action::TalkStart]);
        assert!(s.update(&b(Buttons::ASSISTANT), 600).is_empty(), "TalkStart once");
        assert_eq!(s.update(&b(Buttons::empty()), 900), vec![Action::TalkEnd]);
    }

    #[test]
    fn hold_fires_from_timeout_without_new_report() {
        let mut s = ov();
        let held = b(Buttons::ASSISTANT);
        s.update(&held, 0);
        // The bridge re-feeds the last state when its read times out.
        assert_eq!(s.update(&held, 310), vec![Action::TalkStart]);
    }

    #[test]
    fn switching_the_assistant_choice_clears_a_pending_hold() {
        let mut s = ov();
        s.update(&b(Buttons::ASSISTANT), 0);
        s.set(AssistantAction::Off, CaptureAction::TapHold, StadiaAction::GuideHold);
        s.set(AssistantAction::Overlay, CaptureAction::TapHold, StadiaAction::GuideHold);
        assert!(s.update(&b(Buttons::empty()), 50).is_empty());
    }

    #[test]
    fn saving_unrelated_settings_keeps_a_pending_assistant_hold() {
        // Any settings save (e.g. the overlay remembering its position) re-applies settings mid-hold.
        let mut s = ov();
        s.update(&b(Buttons::ASSISTANT), 0);
        s.update(&b(Buttons::ASSISTANT), 300); // TalkStart
        s.set(AssistantAction::Overlay, CaptureAction::Off, StadiaAction::GuideOnly);
        assert_eq!(s.update(&b(Buttons::empty()), 900), vec![Action::TalkEnd], "the hold survives");
    }

    #[test]
    fn off_does_nothing() {
        let mut s = Special::new(AssistantAction::Off, CaptureAction::Off, StadiaAction::GuideOnly);
        assert!(s.update(&b(Buttons::ASSISTANT | Buttons::CAPTURE | Buttons::STADIA), 0).is_empty());
        assert!(s.update(&b(Buttons::empty()), 2000).is_empty());
    }

    #[test]
    fn tap_waits_for_a_possible_second_tap() {
        let mut t = TapHold::default();
        assert_eq!(t.update(true, 0), None);
        assert_eq!(t.update(false, 100), None);
        assert_eq!(t.deadline(), Some(400));
        assert!(t.busy());
        assert_eq!(t.update(false, 399), None);
        assert_eq!(t.update(false, 400), Some(Action::OverlayTap));
        assert!(!t.busy());
    }

    #[test]
    fn double_tap_starts_live_at_once() {
        let mut t = TapHold::default();
        t.update(true, 0);
        t.update(false, 100);
        assert_eq!(t.update(true, 250), Some(Action::LiveStart));
        assert_eq!(t.update(false, 300), None, "the second release is not a tap");
        assert_eq!(t.update(false, 1000), None);
        assert!(!t.busy());
    }

    #[test]
    fn hold_still_talks() {
        let mut t = TapHold::default();
        t.update(true, 0);
        assert_eq!(t.update(true, 300), Some(Action::TalkStart));
        assert_eq!(t.update(false, 900), Some(Action::TalkEnd));
        assert_eq!(t.update(false, 2000), None);
    }
}
