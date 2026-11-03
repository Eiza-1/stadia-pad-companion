//! The Gemini overlay: an on-demand, non-activating window (overlay.html) at the lower centre of the
//! screen you're on. Assistant tap = type, hold = talk (live dictation), release = send. Created when
//! summoned, destroyed when closed; nothing of it runs otherwise.

use pad_core::special::Action;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Type,
    Talk,
    /// Gemini Live: the orb panel; the game keeps the pad.
    Live,
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
    /// Show why speech-to-text can't work (and how to fix it) in the pill.
    SpeechError(&'static str),
    /// Give the text field real keyboard focus (the keyboard shortcut was used).
    FocusField,
    /// Start a Gemini Live session (mic, voice, dim window).
    LiveStart,
    /// End it: everything it opened closes.
    LiveEnd,
    /// Speakers mode: Assistant held (`true`) / released, talking over Gemini.
    LiveTalk(bool),
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
    /// A speech error that came before the page could show it.
    speech_error: Option<&'static str>,
    /// The keyboard shortcut opened the window: focus the field once it's ready.
    focus_pending: bool,
    /// A Live session is on.
    live: bool,
}

impl OverlayState {
    pub fn on_action(&mut self, a: Action) -> Vec<Cmd> {
        if self.live {
            return match a {
                Action::OverlayTap => self.on_end_live(),
                Action::TalkStart => vec![Cmd::LiveTalk(true)],
                Action::TalkEnd => vec![Cmd::LiveTalk(false)],
                _ => vec![],
            };
        }
        match a {
            Action::LiveStart => match self.win {
                Win::None | Win::Creating => {
                    let mut out = self.open_or_show(Mode::Live);
                    out.retain(|c| *c == Cmd::Open);
                    out
                }
                Win::Ready => {
                    let held = self.shown.is_some();
                    self.shown = Some(Mode::Live);
                    self.live = true;
                    let mut out = vec![Cmd::Show(Mode::Live)];
                    if held {
                        out.push(Cmd::Capture(false)); // Live never holds the pad
                    }
                    out.push(Cmd::LiveStart);
                    out
                }
            },
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
        if let Some(e) = self.speech_error.take() {
            out.push(Cmd::SpeechError(e));
        }
        if std::mem::take(&mut self.focus_pending) && self.want == Some(Mode::Type) {
            out.push(Cmd::FocusField);
        }
        if std::mem::take(&mut self.send_pending) {
            out.push(Cmd::SendTranscript);
        }
        if self.want == Some(Mode::Live) {
            self.live = true;
            out.push(Cmd::LiveStart);
        }
        out
    }

    /// The player ended Live (End button, Assistant tap).
    pub fn on_end_live(&mut self) -> Vec<Cmd> {
        if !self.live {
            return vec![];
        }
        self.live = false;
        self.shown = None;
        vec![Cmd::LiveEnd, Cmd::Hide]
    }

    /// The session ended on its own (Google closed it, mic error): the panel tucks away.
    pub fn on_live_ended(&mut self) -> Vec<Cmd> {
        if !self.live {
            return vec![];
        }
        self.live = false;
        self.shown = None;
        vec![Cmd::Hide]
    }

    pub fn live(&self) -> bool {
        self.live
    }

    /// The same actions from the keyboard shortcut: a tap also gives the field keyboard focus, since
    /// a keyboard player types on the real keyboard.
    pub fn on_key_action(&mut self, a: Action) -> Vec<Cmd> {
        let mut out = self.on_action(a);
        if a == Action::OverlayTap {
            if out.contains(&Cmd::Show(Mode::Type)) {
                out.push(Cmd::FocusField);
            } else if out.contains(&Cmd::Open) {
                self.focus_pending = true;
            }
        }
        out
    }

    /// Speech-to-text failed: shown now if the page is there, else once it is.
    pub fn on_speech_error(&mut self, e: &'static str) -> Vec<Cmd> {
        if self.win == Win::Ready {
            vec![Cmd::SpeechError(e)]
        } else {
            self.speech_error = Some(e);
            vec![]
        }
    }

    /// Nothing was heard: keep the pill open (and the pad) so the player can type instead.
    pub fn on_empty_transcript(&mut self) -> Vec<Cmd> {
        if self.win != Win::Ready {
            return vec![];
        }
        self.shown = Some(Mode::Type);
        vec![Cmd::Show(Mode::Type)]
    }

    /// The pad disconnected: end a talk in progress, or tuck the pill away and release the pad.
    pub fn on_pad_gone(&mut self) -> Vec<Cmd> {
        if self.live {
            return vec![]; // Live doesn't use the pad
        }
        if self.talking {
            return self.on_action(Action::TalkEnd);
        }
        match self.win {
            Win::Ready if self.shown.is_some() => {
                self.shown = None;
                vec![Cmd::Hide, Cmd::Capture(false)]
            }
            Win::Creating if self.want.is_some() => {
                self.want = None;
                vec![Cmd::Capture(false)]
            }
            _ => vec![],
        }
    }

    /// The page itself showed the pill for typing (e.g. after the key was saved): it needs the pad.
    pub fn on_show_type(&mut self) -> Vec<Cmd> {
        if self.win != Win::Ready {
            return vec![];
        }
        self.shown = Some(Mode::Type);
        vec![Cmd::Show(Mode::Type), Cmd::Capture(true)]
    }

    /// The window was asked for but hasn't reported ready.
    pub fn creating(&self) -> bool {
        self.win == Win::Creating
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
use pad_core::settings::AnswerVoice;
use pad_core::win::live_session::{LiveConfig, LiveSession, LiveUpdate};
use pad_core::win::{keystore, overlay_win, sound, speech::Dictation, speech::SpeechEvent};
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "overlay";
/// The Live dim and Share screen frame: a full-screen window that never takes clicks or focus.
pub const DIM_LABEL: &str = "livedim";
/// Window width in logical px before the page reports its own (it follows the pill size setting:
/// 720 at Standard, the 600 px pill plus room for its glow); the height follows the content.
const WIDTH: f64 = 720.0;
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
    /// Width the page asked for (logical px); 0 = not reported yet.
    width: Mutex<f64>,
    /// The Gemini Live session while Live is on.
    live: Mutex<Option<LiveSession>>,
    /// "Share screen with Live" started it: turn sharing on once the session exists.
    share_on_start: Mutex<bool>,
}

fn lk<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ov(app: &AppHandle) -> tauri::State<'_, Overlay> {
    app.state::<Overlay>()
}

/// Assistant button actions.
pub fn on_action(app: &AppHandle, a: Action) {
    let cmds = lk(&ov(app).state).on_action(a);
    run(app, cmds);
}

/// Keyboard shortcut actions (a tap also focuses the field for real-keyboard typing).
pub fn on_key_action(app: &AppHandle, a: Action) {
    let cmds = lk(&ov(app).state).on_key_action(a);
    run(app, cmds);
}

/// The pad disconnected (bridge status): don't leave the overlay holding a pad that isn't there.
pub fn on_pad_gone(app: &AppHandle) {
    let cmds = lk(&ov(app).state).on_pad_gone();
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
                let d = Dictation::start(move |e| match e {
                    SpeechEvent::Partial(t) => emit(&h, "stt", json!({ "text": t, "final": false })),
                    SpeechEvent::Final(t) => emit(&h, "stt", json!({ "text": t, "final": true })),
                    SpeechEvent::Error(m) => {
                        let cmds = lk(&ov(&h).state).on_speech_error(m);
                        run(&h, cmds);
                    }
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
                        let cmds = lk(&ov(&h).state).on_empty_transcript();
                        run(&h, cmds);
                        emit(&h, "stt", json!({ "text": "", "final": true, "empty": true }));
                    } else {
                        emit(&h, "stt", json!({ "text": text, "final": true }));
                        ask(&h, text);
                    }
                });
            }
            Cmd::Capture(on) => capture(app, on),
            Cmd::SpeechError(m) => emit(app, "stt", json!({ "error": m })),
            Cmd::LiveStart => live_start(app),
            Cmd::LiveEnd => live_teardown(app),
            Cmd::LiveTalk(on) => {
                if let Some(s) = lk(&ov(app).live).as_ref() {
                    s.set_talk_over(on);
                }
            }
            Cmd::FocusField => {
                if let Some(w) = app.get_webview_window(LABEL) {
                    take_focus(&w);
                }
                emit(app, "ov_focus", json!({}));
            }
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
    let width = Some(*lk(&o.width)).filter(|&v| v > 0.0).unwrap_or(WIDTH);
    let (pw, ph) = ((width * scale).round() as i32, (height_logical * scale).round() as i32);
    let anchor = (*lk(&o.anchor)).filter(|&(x, y)| overlay_win::on_screen(x, y - 1));
    let (cx, bottom) = anchor.unwrap_or_else(|| {
        let front = *lk(&o.prev_front);
        let (l, _t, r, b) = overlay_win::work_area(front).unwrap_or((0, 0, 1920, 1080));
        ((l + r) / 2, b - (GAP * scale) as i32)
    });
    let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = w.set_position(PhysicalPosition::new(cx - pw / 2, bottom - ph));
}

/// Pill size / animation changed in Settings: an open overlay redraws (and reports its new size).
pub fn restyle(app: &AppHandle) {
    emit(app, "ov_style", style(app));
}

fn style(app: &AppHandle) -> serde_json::Value {
    let s = app.state::<AppState>().with_bridge(|b| b.settings()).unwrap_or_default();
    json!({ "scale": s.overlay_scale, "anim": s.overlay_anim, "accent": overlay_win::accent() })
}

fn open(app: &AppHandle) {
    let o = ov(app);
    *lk(&o.width) = 0.0;
    pad_core::win::apps::warm();
    *lk(&o.prev_front) = overlay_win::foreground();
    *lk(&o.anchor) = app.state::<AppState>().with_bridge(|b| b.settings().overlay_pos).flatten();
    // If the page never reports ready (WebView2 fault), don't keep the pad from games.
    let w = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        if lk(&ov(&w).state).creating() {
            pad_core::log!("overlay never became ready; closing it");
            if let Some(win) = w.get_webview_window(LABEL) {
                let _ = win.destroy();
            }
            closed(&w);
        }
    });
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
    sound::stop();
    live_teardown(app);
    if let Some(d) = lk(&o.listening).take() {
        std::thread::spawn(move || drop(d.stop()));
    }
    capture(app, false);
}

/// Asks Gemini with the conversation so far; streams `answer` events to the overlay.
fn ask(app: &AppHandle, text: String) {
    let o = ov(app);
    lk(&o.state).on_sent();
    sound::stop(); // a new question cuts off the last answer's voice
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
    // PC commands ("open Spotify", "volume up") run here at once; Gemini can't touch the PC.
    if let Some(cmd) = pad_core::pccmd::parse(&text) {
        lk(&o.history).pop();
        let h = app.clone();
        std::thread::spawn(move || match pad_core::win::apps::run(&cmd) {
            Ok(line) => {
                emit(&h, "answer", json!({ "id": id, "delta": line }));
                emit(&h, "answer", json!({ "id": id, "done": true }));
            }
            Err(e) => emit(&h, "answer", json!({ "id": id, "error": e })),
        });
        return;
    }
    let h = app.clone();
    let _ = std::thread::Builder::new().name("gemini-ask".into()).spawn(move || {
        let Some(key) = keystore::get() else {
            lk(&ov(&h).history).pop();
            emit(&h, "answer", json!({ "id": id, "error": "nokey" }));
            return;
        };
        match gemini::ask_stream(&key, &history, |d| emit(&h, "answer", json!({ "id": id, "delta": d }))) {
            Ok(full) => {
                lk(&ov(&h).history).push(Turn { user: false, text: full.clone() });
                emit(&h, "answer", json!({ "id": id, "done": true }));
                read_aloud(&h, id, &key, &full);
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

/// Voice or Both: Gemini reads the answer in the chosen voice. The card learns how long it will speak
/// (so it stays up), or that the voice failed (so Voice-only shows the text instead).
fn read_aloud(app: &AppHandle, id: u64, key: &str, text: &str) {
    let s = app.state::<AppState>().with_bridge(|b| b.settings()).unwrap_or_default();
    if s.answer_voice == AnswerVoice::Text {
        return;
    }
    match gemini::speak(key, text, &s.voice) {
        Ok(wav) => {
            let ms = gemini::wav_duration_ms(&wav).unwrap_or(0);
            if sound::play(wav) {
                emit(app, "answer", json!({ "id": id, "speaking": ms }));
            } else {
                emit(app, "answer", json!({ "id": id, "voice_error": "Couldn't play the voice" }));
            }
        }
        Err(e) => {
            pad_core::log!("gemini voice: {e:?}");
            emit(app, "answer", json!({ "id": id, "voice_error": e.message() }));
        }
    }
}

// ---- Gemini Live ----

/// Starts the session for the game's screen, with the dim window under the panel. No key yet: the
/// page shows the key card instead (it asks for typing mode).
fn live_start(app: &AppHandle) {
    let o = ov(app);
    let Some(key) = keystore::get() else {
        emit(app, "live", json!({ "state": "nokey" }));
        let cmds = lk(&o.state).on_live_ended();
        run(app, cmds);
        return;
    };
    let s = app.state::<AppState>().with_bridge(|b| b.settings()).unwrap_or_default();
    let front = *lk(&o.prev_front);
    if let Some(w) = app.get_webview_window(LABEL) {
        if let Ok(h) = w.hwnd() {
            overlay_win::exclude_from_capture(h.0 as isize); // Gemini never sees the panel
        }
    }
    open_dim(app, front);
    let h = app.clone();
    let session = LiveSession::start(
        LiveConfig { key, voice: s.voice.clone(), listen: s.live_listen, monitor_of: front, play: true },
        move |u| live_update(&h, u),
    );
    if std::mem::take(&mut *lk(&o.share_on_start)) {
        session.set_sharing(true);
        emit_dim(app, json!({ "sharing": true }));
        emit(app, "live", json!({ "sharing": true }));
    }
    *lk(&o.live) = Some(session);
}

fn live_update(app: &AppHandle, u: LiveUpdate) {
    let payload = match u {
        LiveUpdate::Connecting => json!({ "state": "connecting" }),
        LiveUpdate::Ready => json!({ "state": "listening" }),
        LiveUpdate::Level { you, gemini } => json!({ "level_you": you, "level_gemini": gemini }),
        LiveUpdate::YouSaid(t) => json!({ "you": t }),
        LiveUpdate::GeminiSaid(t) => json!({ "gemini": t }),
        LiveUpdate::TurnDone => json!({ "turn": true }),
        LiveUpdate::Speaking(on) => json!({ "speaking": on }),
        LiveUpdate::Ended(why) => {
            emit(app, "live", json!({ "state": "ended", "error": why }));
            let cmds = lk(&ov(app).state).on_live_ended();
            run(app, cmds);
            live_teardown(app);
            return;
        }
    };
    emit(app, "live", payload);
}

/// Closes everything Live opened. The session is dropped on its own thread: its drop waits for the
/// session thread, which may be the one calling this.
fn live_teardown(app: &AppHandle) {
    let o = ov(app);
    if let Some(s) = lk(&o.live).take() {
        std::thread::spawn(move || drop(s));
    }
    *lk(&o.share_on_start) = false;
    if let Some(w) = app.get_webview_window(DIM_LABEL) {
        let _ = w.destroy();
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        if let Ok(h) = w.hwnd() {
            overlay_win::include_in_capture(h.0 as isize);
            overlay_win::round_region(h.0 as isize, None);
        }
        let _ = w.set_effects(None);
    }
}

fn emit_dim(app: &AppHandle, payload: serde_json::Value) {
    if let Some(w) = app.get_webview_window(DIM_LABEL) {
        let _ = w.emit("dim", payload);
    }
}

/// The dim (and, while sharing, the red frame) over the whole screen the game is on.
fn open_dim(app: &AppHandle, front: isize) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if h.get_webview_window(DIM_LABEL).is_some() {
            return;
        }
        let (l, t, r, b) = overlay_win::monitor_rect(front).unwrap_or((0, 0, 1920, 1080));
        let built = WebviewWindowBuilder::new(&h, DIM_LABEL, WebviewUrl::App("livedim.html".into()))
            .title("Gemini Live")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .visible(false)
            .build();
        match built {
            Ok(w) => {
                let _ = w.set_position(PhysicalPosition::new(l, t));
                let _ = w.set_size(PhysicalSize::new((r - l) as u32, (b - t) as u32));
                let _ = w.set_ignore_cursor_events(true);
                if let Ok(hw) = w.hwnd() {
                    overlay_win::set_no_activate(hw.0 as isize, true);
                    overlay_win::exclude_from_capture(hw.0 as isize);
                }
                let _ = w.show();
                if let Some(ovw) = h.get_webview_window(LABEL) {
                    if let Ok(hw) = ovw.hwnd() {
                        overlay_win::raise(hw.0 as isize); // the panel stays above its dim
                    }
                }
            }
            Err(e) => pad_core::log!("live dim window failed: {e}"),
        }
    });
}

// ---- commands called by overlay.js ----

/// The pill's waveform button (`share` false) or "Share screen with Live" (`true`).
#[tauri::command]
pub fn live_start_cmd(app: AppHandle, share: bool) {
    *lk(&ov(&app).share_on_start) = share;
    let cmds = lk(&ov(&app).state).on_action(Action::LiveStart);
    run(&app, cmds);
}

#[tauri::command]
pub fn live_mute(app: AppHandle, on: bool) {
    if let Some(s) = lk(&ov(&app).live).as_ref() {
        s.set_muted(on);
    }
}

#[tauri::command]
pub fn live_share(app: AppHandle, on: bool) {
    if let Some(s) = lk(&ov(&app).live).as_ref() {
        s.set_sharing(on);
    }
    emit_dim(&app, json!({ "sharing": on }));
}

#[tauri::command]
pub fn live_end(app: AppHandle) {
    let cmds = lk(&ov(&app).state).on_end_live();
    run(&app, cmds);
}

/// The Live panel's place in the window (logical px) and the glass setting: frosted glass (Windows'
/// acrylic) behind the panel only, clipped to its rounded corners; Light glass has none.
#[tauri::command]
pub fn live_glass(window: tauri::WebviewWindow, rect: [f64; 4], radius: f64, frosted: bool) {
    let Ok(h) = window.hwnd() else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    if frosted {
        let p = |v: f64| (v * scale).round() as i32;
        overlay_win::round_region(h.0 as isize, Some((p(rect[0]), p(rect[1]), p(rect[2]), p(rect[3]), p(radius))));
        let _ = window.set_effects(tauri::window::EffectsBuilder::new().effect(tauri::window::Effect::Acrylic).build());
    } else {
        overlay_win::round_region(h.0 as isize, None);
        let _ = window.set_effects(None);
    }
}

/// A card was closed while it was being read: stop the voice.
#[tauri::command]
pub fn overlay_hush() {
    sound::stop();
}

/// Settings › Gemini › Voice: Gemini's voices (name, description).
#[tauri::command]
pub fn gemini_voices() -> Vec<(&'static str, &'static str)> {
    gemini::VOICES.to_vec()
}

/// Settings › Gemini › Voice › Play: a short sample in `voice` (it uses the key's free quota).
#[tauri::command]
pub async fn gemini_voice_sample(voice: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = keystore::get().ok_or("Add your Gemini key first")?;
        let wav = gemini::speak(&key, "Hi! This is how I'll read your answers in game.", &voice).map_err(|e| e.message().to_string())?;
        sound::play(wav).then_some(()).ok_or_else(|| "Couldn't play the voice".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn set_width(app: &AppHandle, width: Option<f64>) {
    if let Some(w) = width.filter(|w| w.is_finite() && *w >= 200.0 && *w <= 2000.0) {
        *lk(&ov(app).width) = w;
    }
}

/// Pill size, animation and the Windows accent colour for the page.
#[tauri::command]
pub fn overlay_style(app: AppHandle) -> serde_json::Value {
    style(&app)
}

/// The page has drawn itself at `width` x `height` (logical px): size, place and show without focus.
#[tauri::command]
pub fn overlay_ready(app: AppHandle, window: tauri::WebviewWindow, height: f64, width: Option<f64>) {
    set_width(&app, width);
    place(&app, &window, height);
    let _ = window.show();
    let cmds = lk(&ov(&app).state).on_window_ready();
    run(&app, cmds);
}

/// The content changed height: resize, keeping the bottom edge where it is.
#[tauri::command]
pub fn overlay_resize(app: AppHandle, window: tauri::WebviewWindow, height: f64, width: Option<f64>) {
    set_width(&app, width);
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

fn take_focus(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        overlay_win::set_no_activate(hwnd.0 as isize, false);
        overlay_win::focus(hwnd.0 as isize);
    }
}

/// The player clicked into the text field: let the window take focus for real-keyboard typing.
#[tauri::command]
pub fn overlay_focus(window: tauri::WebviewWindow) {
    take_focus(&window);
}

/// The page showed the pill for typing itself (no key yet, key just saved): it needs the pad.
#[tauri::command]
pub fn overlay_show_type(app: AppHandle) {
    let cmds = lk(&ov(&app).state).on_show_type();
    run(&app, cmds);
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
    fn speech_error_before_the_page_loads_is_shown_once_ready() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_speech_error("Turn on Online speech recognition");
        assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Talk), Cmd::SpeechError("Turn on Online speech recognition")]);
    }

    #[test]
    fn nothing_heard_leaves_the_pill_open_for_typing() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        s.on_action(Action::TalkEnd);
        assert_eq!(s.on_empty_transcript(), vec![Cmd::Show(Mode::Type)], "pad stays with the overlay");
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Hide, Cmd::Capture(false)]);
    }

    #[test]
    fn pad_gone_while_typing_hands_back_and_tucks_away() {
        let mut s = OverlayState::default();
        s.on_action(Action::OverlayTap);
        s.on_window_ready();
        assert_eq!(s.on_pad_gone(), vec![Cmd::Hide, Cmd::Capture(false)]);
        assert_eq!(s.on_pad_gone(), vec![], "once");
    }

    #[test]
    fn pad_gone_mid_hold_ends_the_talk() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        assert_eq!(s.on_pad_gone(), vec![Cmd::SendTranscript]);
    }

    #[test]
    fn page_showing_the_pill_takes_the_pad() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        s.on_sent();
        assert_eq!(s.on_show_type(), vec![Cmd::Show(Mode::Type), Cmd::Capture(true)]);
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Hide, Cmd::Capture(false)], "tap now hides it");
    }

    #[test]
    fn keyboard_shortcut_tap_focuses_the_field() {
        let mut s = OverlayState::default();
        assert_eq!(s.on_key_action(Action::OverlayTap), vec![Cmd::Open, Cmd::Capture(true)]);
        assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Type), Cmd::FocusField]);
        s.on_sent();
        assert_eq!(s.on_key_action(Action::OverlayTap), vec![Cmd::Show(Mode::Type), Cmd::Capture(true), Cmd::FocusField]);
        let mut p = OverlayState::default();
        p.on_action(Action::OverlayTap);
        assert_eq!(p.on_window_ready(), vec![Cmd::Show(Mode::Type)], "the pad's tap doesn't take focus");
    }

    #[test]
    fn creating_is_reported_until_ready() {
        let mut s = OverlayState::default();
        assert!(!s.creating());
        s.on_action(Action::OverlayTap);
        assert!(s.creating());
        s.on_window_ready();
        assert!(!s.creating());
    }

    #[test]
    fn closed_resets_everything() {
        let mut s = OverlayState::default();
        s.on_action(Action::TalkStart);
        s.on_window_ready();
        s.on_closed();
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Open, Cmd::Capture(true)]);
    }

    fn ready() -> OverlayState {
        let mut s = OverlayState::default();
        s.on_action(Action::OverlayTap);
        s.on_window_ready();
        s
    }

    #[test]
    fn double_tap_opens_live() {
        let mut s = OverlayState::default();
        assert_eq!(s.on_action(Action::LiveStart), vec![Cmd::Open]);
        assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Live), Cmd::LiveStart]);
        let mut s = ready(); // the pill is up for typing and holds the pad
        assert_eq!(s.on_action(Action::LiveStart), vec![Cmd::Show(Mode::Live), Cmd::Capture(false), Cmd::LiveStart]);
        assert_eq!(s.on_action(Action::LiveStart), vec![], "already live");
    }

    #[test]
    fn tap_during_live_ends_it() {
        let mut s = ready();
        s.on_action(Action::LiveStart);
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::LiveEnd, Cmd::Hide]);
        assert!(!s.live());
        assert_eq!(s.on_action(Action::OverlayTap), vec![Cmd::Show(Mode::Type), Cmd::Capture(true)], "a tap types again");
    }

    #[test]
    fn hold_during_live_talks_over() {
        let mut s = ready();
        s.on_action(Action::LiveStart);
        assert_eq!(s.on_action(Action::TalkStart), vec![Cmd::LiveTalk(true)]);
        assert_eq!(s.on_action(Action::TalkEnd), vec![Cmd::LiveTalk(false)]);
    }

    #[test]
    fn end_live_stops_everything() {
        let mut s = ready();
        s.on_action(Action::LiveStart);
        assert_eq!(s.on_end_live(), vec![Cmd::LiveEnd, Cmd::Hide]);
        assert_eq!(s.on_end_live(), vec![], "once");
        s.on_action(Action::LiveStart);
        s.on_closed();
        assert!(!s.live());
    }

    #[test]
    fn server_close_ends_live_with_a_message() {
        let mut s = ready();
        s.on_action(Action::LiveStart);
        assert_eq!(s.on_live_ended(), vec![Cmd::Hide]);
        assert!(!s.live());
        assert_eq!(s.on_action(Action::LiveStart), vec![Cmd::Show(Mode::Live), Cmd::LiveStart], "can start again");
    }

    #[test]
    fn pad_gone_does_not_end_live() {
        let mut s = ready();
        s.on_action(Action::LiveStart);
        assert_eq!(s.on_pad_gone(), vec![]);
        assert!(s.live());
    }
}
