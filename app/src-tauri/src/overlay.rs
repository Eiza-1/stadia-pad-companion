//! The Gemini overlay: an on-demand, non-activating window (overlay.html) at the lower centre of the
//! screen you're on. Assistant tap = type, hold = talk (live dictation), release = send. Created when
//! summoned, destroyed when closed; nothing of it runs otherwise.

use pad_core::special::Action;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Type,
    Talk,
}

/// What the overlay glue must do in response to a button action or a window event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    /// Create the window (it reports ready once drawn).
    Open,
    /// Show the pill in this mode.
    Show(Mode),
    /// Tuck the pill away (answer cards may stay).
    Hide,
    StartListening,
    /// Stop listening and ask Gemini with what was heard.
    SendTranscript,
    /// Hand the pad to the overlay (`true`) or back to games.
    Capture(bool),
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum Win {
    #[default]
    None,
    Creating,
    Ready,
}

/// The overlay's open / talk / send logic, kept free of windows so it can be tested.
#[derive(Default)]
pub struct OverlayState {
    win: Win,
    /// The pill as shown (`None` = tucked away or not yet drawn).
    shown: Option<Mode>,
    /// The mode to show once the window is ready.
    want: Option<Mode>,
    talking: bool,
    send_pending: bool,
}

impl OverlayState {
    pub fn on_action(&mut self, a: Action) -> Vec<Cmd> {
        match a {
            Action::TalkStart => {
                self.talking = true;
                let mut out = self.open_or_show(Mode::Talk);
                out.extend([Cmd::StartListening, Cmd::Capture(true)]);
                out
            }
            Action::TalkEnd if self.talking => {
                self.talking = false;
                if self.win == Win::Ready {
                    vec![Cmd::SendTranscript]
                } else {
                    self.send_pending = true;
                    vec![]
                }
            }
            Action::OverlayTap if self.win == Win::Ready && self.shown == Some(Mode::Type) => {
                self.shown = None;
                vec![Cmd::Hide, Cmd::Capture(false)]
            }
            Action::OverlayTap => {
                let mut out = self.open_or_show(Mode::Type);
                if !out.is_empty() {
                    out.push(Cmd::Capture(true));
                }
                out
            }
            _ => vec![],
        }
    }

    fn open_or_show(&mut self, m: Mode) -> Vec<Cmd> {
        self.want = Some(m);
        match self.win {
            Win::None => {
                self.win = Win::Creating;
                vec![Cmd::Open]
            }
            Win::Creating => vec![],
            Win::Ready => {
                self.shown = Some(m);
                vec![Cmd::Show(m)]
            }
        }
    }

    /// The window has drawn itself for the first time.
    pub fn on_window_ready(&mut self) -> Vec<Cmd> {
        self.win = Win::Ready;
        self.shown = self.want;
        let mut out: Vec<Cmd> = self.want.map(Cmd::Show).into_iter().collect();
        if std::mem::take(&mut self.send_pending) {
            out.push(Cmd::SendTranscript);
        }
        out
    }

    /// A question went to Gemini: the pill tucks away and the pad goes back to the game.
    pub fn on_sent(&mut self) {
        self.shown = None;
    }

    pub fn on_closed(&mut self) {
        *self = OverlayState::default();
    }
}


// ---------------------------------------------------------------------------------------------------
// Glue: the window, dictation, Gemini and the pad hand-off.

use crate::AppState;
use pad_core::gemini::{self, Turn};
use pad_core::win::{keystore, overlay_win, speech::Dictation, speech::SpeechEvent};
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "overlay";
/// Window width in logical px (overlay.css lays out for this); the height follows the content.
const WIDTH: f64 = 760.0;
/// Gap between the window's bottom edge and the taskbar, logical px.
const GAP: f64 = 24.0;

#[derive(Default)]
pub struct Overlay {
    state: Mutex<OverlayState>,
    listening: Mutex<Option<Dictation>>,
    history: Mutex<Vec<Turn>>,
    /// The window in front when the overlay opened; focus goes back there if the overlay took it.
    prev_front: Mutex<isize>,
    /// Bottom-centre anchor of the window (physical px).
    anchor: Mutex<Option<(i32, i32)>>,
    next_id: Mutex<u64>,
}

fn lk<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ov(app: &AppHandle) -> tauri::State<'_, Overlay> {
    app.state::<Overlay>()
}

/// Assistant button / keyboard shortcut actions.
pub fn on_action(app: &AppHandle, a: Action) {
    let cmds = lk(&ov(app).state).on_action(a);
    run(app, cmds);
}

fn run(app: &AppHandle, cmds: Vec<Cmd>) {
    for c in cmds {
        match c {
            Cmd::Open => open(app),
            Cmd::Show(m) => emit(app, "ov_mode", json!({ "mode": m })),
            Cmd::Hide => emit(app, "ov_mode", json!({ "mode": "hidden" })),
            Cmd::StartListening => {
                let h = app.clone();
                let d = Dictation::start(move |e| {
                    let v = match e {
                        SpeechEvent::Partial(t) => json!({ "text": t, "final": false }),
                        SpeechEvent::Final(t) => json!({ "text": t, "final": true }),
                        SpeechEvent::Error(m) => json!({ "error": m }),
                    };
                    emit(&h, "stt", v);
                });
                *lk(&ov(app).listening) = Some(d);
            }
            Cmd::SendTranscript => {
                let d = lk(&ov(app).listening).take();
                let h = app.clone();
                // stop() waits up to 1.5 s for the last words: never on the dispatcher thread.
                std::thread::spawn(move || {
                    let text = d.map(|d| d.stop()).unwrap_or_default();
                    if text.trim().is_empty() {
                        emit(&h, "stt", json!({ "text": "", "final": true, "empty": true }));
                        lk(&ov(&h).state).on_sent();
                        capture(&h, false);
                        emit(&h, "ov_mode", json!({ "mode": "hidden" }));
                    } else {
                        emit(&h, "stt", json!({ "text": text, "final": true }));
                        ask(&h, text);
                    }
                });
            }
            Cmd::Capture(on) => capture(app, on),
        }
    }
}

fn emit(app: &AppHandle, event: &str, payload: serde_json::Value) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.emit(event, payload);
    }
}

fn capture(app: &AppHandle, on: bool) {
    app.state::<AppState>().with_bridge(|b| b.set_capture(on));
}

/// Lower centre of the screen you're on (or where the pill was dragged to).
fn place(app: &AppHandle, w: &tauri::WebviewWindow, height_logical: f64) {
    let o = ov(app);
    let scale = w.scale_factor().unwrap_or(1.0);
    let (pw, ph) = ((WIDTH * scale).round() as i32, (height_logical * scale).round() as i32);
    let anchor = *lk(&o.anchor);
    let (cx, bottom) = anchor.unwrap_or_else(|| {
        let front = *lk(&o.prev_front);
        let (l, _t, r, b) = overlay_win::work_area(front).unwrap_or((0, 0, 1920, 1080));
        ((l + r) / 2, b - (GAP * scale) as i32)
    });
    let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = w.set_position(PhysicalPosition::new(cx - pw / 2, bottom - ph));
}

fn open(app: &AppHandle) {
    let o = ov(app);
    *lk(&o.prev_front) = overlay_win::foreground();
    *lk(&o.anchor) = app.state::<AppState>().with_bridge(|b| b.settings().overlay_pos).flatten();
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if h.get_webview_window(LABEL).is_some() {
            return;
        }
        let built = WebviewWindowBuilder::new(&h, LABEL, WebviewUrl::App("overlay.html".into()))
            .title("Gemini")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .visible(false) // shown by overlay_ready once drawn
            .inner_size(WIDTH, 420.0)
            .build();
        match built {
            Ok(w) => {
                if let Ok(hwnd) = w.hwnd() {
                    overlay_win::set_no_activate(hwnd.0 as isize, true);
                }
                place(&h, &w, 420.0);
                let h2 = h.clone();
                w.on_window_event(move |e| {
                    if let WindowEvent::Destroyed = e {
                        closed(&h2);
                    }
                });
            }
            Err(e) => {
                pad_core::log!("overlay create failed: {e}");
                closed(&h);
            }
        }
    });
}

/// Everything the overlay held goes: mic, conversation, the pad.
fn closed(app: &AppHandle) {
    let o = ov(app);
    lk(&o.state).on_closed();
    lk(&o.history).clear();
    if let Some(d) = lk(&o.listening).take() {
        std::thread::spawn(move || drop(d.stop()));
    }
    capture(app, false);
}

/// Asks Gemini with the conversation so far; streams `answer` events to the overlay.
fn ask(app: &AppHandle, text: String) {
    let o = ov(app);
    lk(&o.state).on_sent();
    capture(app, false);
    emit(app, "ov_mode", json!({ "mode": "hidden" }));
    let id = {
        let mut n = lk(&o.next_id);
        *n += 1;
        *n
    };
    let history = {
        let mut h = lk(&o.history);
        h.push(Turn { user: true, text: text.clone() });
        h.clone()
    };
    emit(app, "answer", json!({ "id": id, "question": text }));
    let h = app.clone();
    let _ = std::thread::Builder::new().name("gemini-ask".into()).spawn(move || {
        let Some(key) = keystore::get() else {
            lk(&ov(&h).history).pop();
            emit(&h, "answer", json!({ "id": id, "error": "nokey" }));
            return;
        };
        match gemini::ask_stream(&key, &history, |d| emit(&h, "answer", json!({ "id": id, "delta": d }))) {
            Ok(full) => {
                lk(&ov(&h).history).push(Turn { user: false, text: full });
                emit(&h, "answer", json!({ "id": id, "done": true }));
            }
            Err(e) => {
                pad_core::log!("gemini: {e:?}");
                // Keep the conversation consistent: drop the unanswered question.
                lk(&ov(&h).history).pop();
                emit(&h, "answer", json!({ "id": id, "error": e.message() }));
            }
        }
    });
}

// ---- commands called by overlay.js ----

/// The page has drawn itself at `height` (logical px): size, place and show without focus.
#[tauri::command]
pub fn overlay_ready(app: AppHandle, window: tauri::WebviewWindow, height: f64) {
    place(&app, &window, height);
    let _ = window.show();
    let cmds = lk(&ov(&app).state).on_window_ready();
    run(&app, cmds);
}

/// The content changed height: resize, keeping the bottom edge where it is.
#[tauri::command]
pub fn overlay_resize(app: AppHandle, window: tauri::WebviewWindow, height: f64) {
    place(&app, &window, height);
}

#[tauri::command]
pub fn overlay_ask(app: AppHandle, text: String) {
    if !text.trim().is_empty() {
        ask(&app, text.trim().to_string());
    }
}

/// The page opened (`true`) or tucked away (`false`) the pill itself, e.g. B on an empty field.
#[tauri::command]
pub fn overlay_capture(app: AppHandle, on: bool) {
    if !on {
        lk(&ov(&app).state).on_sent();
    }
    capture(&app, on);
}

/// Closes the overlay; if it had taken focus, the app that was in front gets it back.
#[tauri::command]
pub fn overlay_close(app: AppHandle, window: tauri::WebviewWindow) {
    let front = *lk(&ov(&app).prev_front);
    let had_focus = window.is_focused().unwrap_or(false);
    let _ = window.destroy();
    if had_focus {
        overlay_win::focus(front);
    }
}

/// The player clicked into the text field: let the window take focus for real-keyboard typing.
#[tauri::command]
pub fn overlay_focus(window: tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        overlay_win::set_no_activate(hwnd.0 as isize, false);
        overlay_win::focus(hwnd.0 as isize);
    }
}

/// The pill was dragged: remember its bottom-centre so it opens there next time.
#[tauri::command]
pub fn overlay_drag_end(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else { return Ok(()) };
    let anchor = (pos.x + size.width as i32 / 2, pos.y + size.height as i32);
    *lk(&ov(&app).anchor) = Some(anchor);
    crate::ipc::update_settings(&app.state::<AppState>(), |s| s.overlay_pos = Some(anchor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pad_core::special::Action;

    #[test]
    fn talk_end_before_window_ready_still_sends() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_action(Action::TalkEnd);
        assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Talk), Cmd::SendTranscript]);
    }

    #[test]
    fn talk_start_listens_and_takes_the_pad_at_once() {
        let mut s = OverlayState::default();
        assert_eq!(s.on_action(Action::TalkStart), vec![Cmd::Open, Cmd::StartListening, Cmd::Capture(true)]);
    }

    #[test]
    fn talk_end_with_window_ready_sends_now() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        assert_eq!(s.on_action(Action::TalkEnd), vec![Cmd::SendTranscript]);
    }

    #[test]
    fn tap_opens_typing_then_tap_hides() {
        let mut s = OverlayState::default();
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Open, Cmd::Capture(true)]);
        assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Type)]);
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Hide, Cmd::Capture(false)]);
    }

    #[test]
    fn tap_after_send_opens_typing_again() {
        let mut s = OverlayState::default();
        s.on_action(Action::OverlayTap);
        s.on_window_ready();
        s.on_sent();
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Show(Mode::Type), Cmd::Capture(true)]);
    }

    #[test]
    fn closed_resets_everything() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        s.on_closed();
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Open, Cmd::Capture(true)]);
    }
}
