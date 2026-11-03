//! One Gemini Live session on its own thread: the WebSocket to Google, the mic, Gemini's voice and
//! Share screen frames. The app sees only `LiveUpdate`s; the key never leaves Rust. Everything the
//! session opened closes when its thread returns (End, drop, or Google ending it).

use crate::live::{
    audio_msg, frame_due, frame_msg, level, mic_open, parse_server, setup_msg, ws_url, LiveEvent, ECHO_TAIL_MS, LIVE_FALLBACK,
    LIVE_MODEL,
};
use crate::log;
use crate::settings::LiveListen;
use crate::win::audio::{Mic, Player};
use crate::win::screen::capture_jpeg;
use serde_json::json;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

pub struct LiveConfig {
    pub key: String,
    pub voice: String,
    pub listen: LiveListen,
    /// A window on the screen to share (the game / the overlay); 0 = the primary screen.
    pub monitor_of: isize,
    /// Play Gemini's voice (false only for silent probes).
    pub play: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveUpdate {
    Connecting,
    Ready,
    /// Loudness 0..1 of the player's mic (while it is sent) and of Gemini's voice, about 30 times a second.
    Level { you: f32, gemini: f32 },
    YouSaid(String),
    GeminiSaid(String),
    TurnDone,
    /// Gemini started / stopped speaking (Speakers mode: the mic is closed while it speaks).
    Speaking(bool),
    /// The session is over; `Some` says why when it wasn't the player's choice.
    Ended(Option<String>),
}

#[derive(Default)]
struct Ctl {
    muted: AtomicBool,
    talk_over: AtomicBool,
    sharing: AtomicBool,
    ended: AtomicBool,
    text: Mutex<Vec<String>>,
}

pub struct LiveSession {
    ctl: Arc<Ctl>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const READ_POLL: Duration = Duration::from_millis(20);
const LEVEL_EVERY: Duration = Duration::from_millis(33);
const SETUP_WAIT: Duration = Duration::from_secs(10);
const FRAME_LONG_SIDE: u32 = 1024;

pub const MSG_LIMIT: &str = "Gemini ended the Live session (free limit). Double-tap Assistant to start again.";
pub const MSG_OFFLINE: &str = "No internet connection";
pub const MSG_KEY: &str = "Your Gemini key isn't working. Check it in Settings";
pub const MSG_LOST: &str = "Gemini Live stopped. Double-tap Assistant to start again.";

/// The line shown when Google closes the session, from its close reason.
pub fn end_message(reason: &str) -> &'static str {
    let r = reason.to_lowercase();
    if r.contains("quota") || r.contains("exceed") || r.contains("resource_exhausted") || r.contains("limit") {
        MSG_LIMIT
    } else if r.contains("api key") || r.contains("api_key") || r.contains("permission") || r.contains("unauthenticated") {
        MSG_KEY
    } else {
        MSG_LOST
    }
}

impl LiveSession {
    pub fn start(cfg: LiveConfig, on: impl Fn(LiveUpdate) + Send + Sync + 'static) -> LiveSession {
        let ctl = Arc::new(Ctl::default());
        let c = ctl.clone();
        let thread = std::thread::Builder::new()
            .name("live-session".into())
            .spawn(move || {
                let why = run(&cfg, &c, &on);
                on(LiveUpdate::Ended(why));
            })
            .ok();
        LiveSession { ctl, thread }
    }

    pub fn set_muted(&self, on: bool) {
        self.ctl.muted.store(on, Ordering::Relaxed);
    }
    /// Speakers mode: holding Assistant opens the mic over Gemini's voice.
    pub fn set_talk_over(&self, on: bool) {
        self.ctl.talk_over.store(on, Ordering::Relaxed);
    }
    pub fn set_sharing(&self, on: bool) {
        self.ctl.sharing.store(on, Ordering::Relaxed);
    }
    /// A typed line, as if spoken.
    pub fn send_text(&self, text: &str) {
        self.ctl.text.lock().unwrap_or_else(|e| e.into_inner()).push(text.to_string());
    }
    /// Ends the session; returns once everything is closed.
    pub fn end(mut self) {
        self.stop();
    }
    fn stop(&mut self) {
        self.ctl.ended.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.stop();
    }
}

enum ConnErr {
    /// Try the next model / without grounding.
    Retry(String),
    /// Google said "quota": the free tier refuses Search in Live, so this may pass without it.
    Limit,
    Fatal(&'static str),
}

/// The free tier refused Search grounding in Live this run: later sessions skip that try.
static GROUNDING_REFUSED: AtomicBool = AtomicBool::new(false);

fn connect(key: &str, model: &str, voice: &str, grounded: bool, resume: Option<&str>) -> Result<Ws, ConnErr> {
    let (mut ws, _) = tungstenite::connect(ws_url(key)).map_err(|e| match e {
        tungstenite::Error::Http(r) => match r.status().as_u16() {
            401 | 403 => ConnErr::Fatal(MSG_KEY),
            429 => ConnErr::Limit,
            s => ConnErr::Retry(format!("http {s}")),
        },
        tungstenite::Error::Io(_) | tungstenite::Error::Tls(_) => ConnErr::Fatal(MSG_OFFLINE),
        other => ConnErr::Retry(other.to_string().replace(key, "***")), // never log the key
    })?;
    ws.send(Message::Text(setup_msg(model, voice, grounded, resume).to_string().into()))
        .map_err(|e| ConnErr::Retry(e.to_string().replace(key, "***")))?;
    set_poll(&mut ws, Some(SETUP_WAIT));
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                if parse_server(&t).contains(&LiveEvent::SetupComplete) {
                    break;
                }
            }
            Ok(Message::Binary(b)) => {
                if parse_server(&String::from_utf8_lossy(&b)).contains(&LiveEvent::SetupComplete) {
                    break;
                }
            }
            Ok(Message::Close(c)) => {
                let reason = c.map(|f| f.reason.to_string()).unwrap_or_default();
                log!("live: {model} closed during setup: {reason}");
                return Err(match end_message(&reason) {
                    MSG_LIMIT => ConnErr::Limit,
                    MSG_KEY => ConnErr::Fatal(MSG_KEY),
                    _ => ConnErr::Retry(reason),
                });
            }
            Ok(_) => {}
            Err(e) => return Err(ConnErr::Retry(e.to_string().replace(key, "***"))),
        }
    }
    set_poll(&mut ws, Some(READ_POLL));
    Ok(ws)
}

fn set_poll(ws: &mut Ws, t: Option<Duration>) {
    let _ = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(t),
        MaybeTlsStream::Rustls(s) => s.get_mut().set_read_timeout(t),
        _ => Ok(()),
    };
}

/// Tries Flash Live with Search, then without, then the fallback model.
fn open(cfg: &LiveConfig, resume: Option<&str>) -> Result<Ws, &'static str> {
    let mut last = String::new();
    let mut limited = false;
    for (model, grounded) in [(LIVE_MODEL, true), (LIVE_MODEL, false), (LIVE_FALLBACK, false)] {
        if grounded && GROUNDING_REFUSED.load(Ordering::Relaxed) {
            continue;
        }
        match connect(&cfg.key, model, &cfg.voice, grounded, resume) {
            Ok(ws) => {
                log!("live: connected to {model} (search {grounded})");
                return Ok(ws);
            }
            Err(ConnErr::Fatal(m)) => return Err(m),
            Err(ConnErr::Limit) => {
                if grounded {
                    GROUNDING_REFUSED.store(true, Ordering::Relaxed);
                } else {
                    limited = true;
                }
            }
            Err(ConnErr::Retry(why)) => last = why,
        }
    }
    log!("live: no model accepted the session: {last}");
    Err(if limited { MSG_LIMIT } else { MSG_LOST })
}

fn is_timeout(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

fn run(cfg: &LiveConfig, ctl: &Ctl, on: &(impl Fn(LiveUpdate) + Send + Sync)) -> Option<String> {
    on(LiveUpdate::Connecting);
    let player = if cfg.play {
        match Player::start() {
            Ok(p) => Some(p),
            Err(e) => return Some(e),
        }
    } else {
        None
    };
    let (mic_tx, mic_rx) = mpsc::channel::<Vec<i16>>();
    let _mic = match Mic::start(move |c| drop(mic_tx.send(c))) {
        Ok(m) => m,
        Err(e) => return Some(e),
    };
    let mut ws = match open(cfg, None) {
        Ok(ws) => ws,
        Err(m) => return Some(m.to_string()),
    };
    on(LiveUpdate::Ready);
    let t0 = Instant::now();
    let ms = || t0.elapsed().as_millis() as u64;
    let (mut last_audio, mut last_frame, mut last_level) = (None::<u64>, None::<u64>, Instant::now());
    let (mut you, mut speaking, mut handle) = (0.0f32, false, None::<String>);
    loop {
        if ctl.ended.load(Ordering::Relaxed) {
            let _ = ws.close(None);
            let _ = ws.flush();
            return None;
        }
        let now = ms();
        if player.as_ref().is_some_and(|p| p.queued_ms() > 0) {
            last_audio = Some(now);
        }
        // Mic: sent only while the gate is open (Speakers: not over Gemini's voice).
        let open_mic = mic_open(cfg.listen, ctl.muted.load(Ordering::Relaxed), ctl.talk_over.load(Ordering::Relaxed), last_audio, now);
        while let Ok(chunk) = mic_rx.try_recv() {
            you = if open_mic { level(&chunk) } else { 0.0 };
            if open_mic && ws.send(Message::Text(audio_msg(&chunk).to_string().into())).is_err() {
                break;
            }
        }
        for t in std::mem::take(&mut *ctl.text.lock().unwrap_or_else(|e| e.into_inner())) {
            let _ = ws.send(Message::Text(json!({ "realtimeInput": { "text": t } }).to_string().into()));
        }
        if frame_due(ctl.sharing.load(Ordering::Relaxed), last_frame, now) {
            last_frame = Some(now);
            if let Some(j) = capture_jpeg(cfg.monitor_of, FRAME_LONG_SIDE) {
                let _ = ws.send(Message::Text(frame_msg(&j).to_string().into()));
            }
        }
        let now_speaking = last_audio.is_some_and(|t| now < t + ECHO_TAIL_MS);
        if now_speaking != speaking {
            speaking = now_speaking;
            on(LiveUpdate::Speaking(speaking));
        }
        if last_level.elapsed() >= LEVEL_EVERY {
            last_level = Instant::now();
            on(LiveUpdate::Level { you, gemini: player.as_ref().map_or(0.0, |p| p.level()) });
        }
        let text = match ws.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => String::from_utf8_lossy(&b).to_string(),
            Ok(Message::Close(c)) => {
                let reason = c.map(|f| f.reason.to_string()).unwrap_or_default();
                log!("live: closed by Google: {reason}");
                match handle.as_deref().map(|h| open(cfg, Some(h))) {
                    Some(Ok(w)) => {
                        ws = w;
                        continue;
                    }
                    _ => return Some(end_message(&reason).to_string()),
                }
            }
            Ok(_) => continue,
            Err(e) if is_timeout(&e) => continue,
            Err(e) => {
                log!("live: connection lost: {e}");
                match handle.as_deref().map(|h| open(cfg, Some(h))) {
                    Some(Ok(w)) => {
                        ws = w;
                        continue;
                    }
                    _ => return Some(MSG_LOST.to_string()),
                }
            }
        };
        for ev in parse_server(&text) {
            match ev {
                LiveEvent::Audio(pcm) => {
                    if let Some(p) = &player {
                        p.push(&pcm);
                    }
                    last_audio = Some(ms());
                }
                LiveEvent::Interrupted => {
                    if let Some(p) = &player {
                        p.clear();
                    }
                }
                LiveEvent::YouSaid(t) => on(LiveUpdate::YouSaid(t)),
                LiveEvent::GeminiSaid(t) => on(LiveUpdate::GeminiSaid(t)),
                LiveEvent::TurnComplete => on(LiveUpdate::TurnDone),
                LiveEvent::Resume(h) => handle = Some(h),
                LiveEvent::GoAway => {
                    // Google is about to close this connection: carry on in a new one.
                    if let Some(Ok(w)) = handle.as_deref().map(|h| open(cfg, Some(h))) {
                        let _ = ws.close(None);
                        ws = w;
                    }
                }
                LiveEvent::Error(m) => return Some(end_message(&m).to_string()),
                LiveEvent::SetupComplete => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_close_reasons_become_one_clear_line() {
        assert_eq!(end_message("You exceeded your current quota"), MSG_LIMIT);
        assert_eq!(end_message("RESOURCE_EXHAUSTED"), MSG_LIMIT);
        assert_eq!(end_message("API key not valid"), MSG_KEY);
        assert_eq!(end_message(""), MSG_LOST);
    }
}
