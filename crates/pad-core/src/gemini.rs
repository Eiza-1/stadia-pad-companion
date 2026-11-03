//! Gemini API client for the overlay: builds requests, streams answers, turns failures into one readable
//! line. The player's own key travels only in the `x-goog-api-key` header.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// The model ids live only here. Flash-Lite answers first: it is fast (about 1 s) and short overlay
/// answers don't need more; Flash is the fallback when Flash-Lite is busy or its free limit is hit.
pub const MODEL: &str = "gemini-flash-lite-latest";
pub const FALLBACK: &str = "gemini-flash-latest";
const API: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub const SYSTEM: &str = "You are a gaming assistant shown in a small overlay over a game. Answer in one to three short sentences; use a compact list only for combos or steps. No preamble.";

#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub user: bool,
    pub text: String,
}

#[derive(Debug)]
pub enum AskError {
    Offline,
    BadKey,
    Quota,
    Busy,
    Other(String),
}

impl AskError {
    /// One line for the answer card.
    pub fn message(&self) -> &'static str {
        match self {
            AskError::Offline => "No internet connection",
            AskError::BadKey => "Your Gemini key isn't working. Check it in Settings",
            AskError::Quota => "Gemini's free limit is used up for now. Try again in a minute",
            AskError::Busy => "Gemini is busy right now. Try again in a moment",
            AskError::Other(_) => "Gemini couldn't answer",
        }
    }
}

/// `status` is `None` when no HTTP response came back at all.
pub fn classify(status: Option<u16>, body: &str) -> AskError {
    match status {
        None => AskError::Offline,
        Some(429) => AskError::Quota,
        Some(500 | 502 | 503 | 504) => AskError::Busy,
        Some(401 | 403) => AskError::BadKey,
        Some(400) if body.contains("API key not valid") || body.contains("API_KEY_INVALID") => AskError::BadKey,
        Some(code) => AskError::Other(format!("HTTP {code}: {}", body.chars().take(200).collect::<String>())),
    }
}

/// A 400 that says the key's tier can't use Google Search grounding: retry without it.
pub fn grounding_unsupported(status: u16, body: &str) -> bool {
    status == 400 && body.contains("google_search")
}

/// One try at an answer: which model, and whether Google Search grounding is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attempt {
    pub model: &'static str,
    pub grounded: bool,
}

/// What to try after `a` failed with `status`, or `None` to give up and show the error.
/// The free tier refuses Search grounding with a 429 (or a 400 naming google_search), so that drops
/// grounding first; a busy or rate-limited Flash-Lite then falls back to Flash.
pub fn next_attempt(a: Attempt, status: u16, body: &str) -> Option<Attempt> {
    if a.grounded && (status == 429 || grounding_unsupported(status, body)) {
        return Some(Attempt { grounded: false, ..a });
    }
    if a.model == MODEL && matches!(status, 429 | 500 | 502 | 503 | 504) {
        return Some(Attempt { model: FALLBACK, ..a });
    }
    None
}

/// When Search grounding was last refused (Unix seconds), so later questions skip the wasted try.
static GROUNDING_REFUSED_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const GROUNDING_RETRY_SECS: u64 = 30 * 60;

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn request_body(history: &[Turn], grounded: bool) -> Value {
    let contents: Vec<Value> = history
        .iter()
        .map(|t| json!({ "role": if t.user { "user" } else { "model" }, "parts": [{ "text": t.text }] }))
        .collect();
    let mut b = json!({ "system_instruction": { "parts": [{ "text": SYSTEM }] }, "contents": contents });
    if grounded {
        b["tools"] = json!([{ "google_search": {} }]);
    }
    b
}

/// The text in one server-sent-events line (`data: {...}`), if it carries any.
pub fn parse_sse_line(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line.strip_prefix("data:")?.trim()).ok()?;
    let parts = v["candidates"][0]["content"]["parts"].as_array()?;
    let text: String = parts.iter().filter_map(|p| p["text"].as_str()).collect();
    (!text.is_empty()).then_some(text)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(12)).build()
}

fn http_error(e: ureq::Error) -> (Option<u16>, String) {
    match e {
        ureq::Error::Status(code, r) => (Some(code), r.into_string().unwrap_or_default()),
        ureq::Error::Transport(t) => (transport_status(t.kind()), String::new()),
    }
}

/// A transport failure after the connection was made (Google accepted it but never answered: an
/// overloaded model) counts as a 504 so it falls back like "busy"; no connection at all is offline.
pub fn transport_status(kind: ureq::ErrorKind) -> Option<u16> {
    matches!(kind, ureq::ErrorKind::Io).then_some(504)
}

/// Streams Gemini's answer to the conversation, calling `on_delta` with each piece; returns the full text.
pub fn ask_stream(key: &str, history: &[Turn], mut on_delta: impl FnMut(&str)) -> Result<String, AskError> {
    use std::sync::atomic::Ordering;
    let refused = GROUNDING_REFUSED_AT.load(Ordering::Relaxed);
    let mut a = Attempt { model: MODEL, grounded: refused == 0 || unix_now() >= refused + GROUNDING_RETRY_SECS };
    let resp = loop {
        let url = format!("{API}/{}:streamGenerateContent?alt=sse", a.model);
        match agent().post(&url).set("x-goog-api-key", key).send_json(request_body(history, a.grounded)) {
            Ok(r) => break r,
            Err(e) => match http_error(e) {
                (Some(code), body) => match next_attempt(a, code, &body) {
                    Some(next) => {
                        if a.grounded && !next.grounded {
                            GROUNDING_REFUSED_AT.store(unix_now(), Ordering::Relaxed);
                        }
                        a = next;
                    }
                    None => return Err(classify(Some(code), &body)),
                },
                (None, b) => return Err(classify(None, &b)),
            },
        }
    };
    let mut full = String::new();
    for line in BufReader::new(resp.into_reader()).lines() {
        let line = line.map_err(|_| AskError::Offline)?;
        if let Some(t) = parse_sse_line(&line) {
            on_delta(&t);
            full.push_str(&t);
        }
    }
    if full.is_empty() {
        return Err(AskError::Other("empty answer".into()));
    }
    Ok(full)
}

/// One cheap request to check a key before saving it.
pub fn validate_key(key: &str) -> Result<(), AskError> {
    agent()
        .get(&format!("{API}/{MODEL}"))
        .set("x-goog-api-key", key)
        .call()
        .map(|_| ())
        .map_err(|e| { let (c, b) = http_error(e); classify(c, &b) })
}

// ---------- read-aloud: Gemini's own natural voices ----------

/// Speech models: Flash-Lite TTS is quicker (about 3 s for a sentence); Flash TTS backs it up.
pub const TTS: &str = "gemini-3.8-flash-lite-tts";
pub const TTS_FALLBACK: &str = "gemini-3.8-flash-tts";
/// Gemini's prebuilt voices offered in Settings (name, how Google describes it).
pub const VOICES: [(&str, &str); 6] =
    [("Aoede", "Breezy"), ("Kore", "Firm"), ("Puck", "Upbeat"), ("Charon", "Informative"), ("Leda", "Youthful"), ("Fenrir", "Excitable")];

/// Only the answer goes in: a style line in front is read aloud too (measured: +5 s of speech).
pub fn tts_body(text: &str, voice: &str) -> Value {
    json!({
        "contents": [{ "parts": [{ "text": text }] }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
        }
    })
}

/// The next speech model after `model` failed with `status`, if any.
pub fn next_tts(model: &str, status: u16) -> Option<&'static str> {
    (model == TTS && matches!(status, 429 | 500 | 502 | 503 | 504)).then_some(TTS_FALLBACK)
}

/// WAV bytes from a speech reply (raw `audio/L16` PCM is wrapped in a WAV header).
pub fn parse_tts(body: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let v: Value = serde_json::from_str(body).ok()?;
    let d = &v["candidates"][0]["content"]["parts"][0]["inlineData"];
    let bytes = base64::engine::general_purpose::STANDARD.decode(d["data"].as_str()?).ok()?;
    let mime = d["mimeType"].as_str().unwrap_or("");
    if mime.starts_with("audio/L16") || mime.contains("pcm") {
        let rate = mime.split(';').find_map(|p| p.trim().strip_prefix("rate=")?.parse().ok()).unwrap_or(24_000);
        Some(pcm_to_wav(&bytes, rate))
    } else {
        Some(bytes)
    }
}

/// 16-bit mono PCM to a WAV file.
pub fn pcm_to_wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

/// Playing time of a WAV file, from its `fmt ` byte rate and `data` size.
pub fn wav_duration_ms(wav: &[u8]) -> Option<u64> {
    if wav.len() < 12 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let (mut i, mut byte_rate, mut data) = (12usize, None, None);
    while i + 8 <= wav.len() {
        let id = &wav[i..i + 4];
        let size = u32::from_le_bytes(wav[i + 4..i + 8].try_into().ok()?) as usize;
        if id == b"fmt " && i + 16 <= wav.len() {
            byte_rate = Some(u32::from_le_bytes(wav.get(i + 16..i + 20)?.try_into().ok()?) as u64);
        }
        if id == b"data" {
            data = Some(size.min(wav.len() - (i + 8)) as u64);
            break;
        }
        i += 8 + size + (size & 1);
    }
    let (r, d) = (byte_rate.filter(|r| *r > 0)?, data?);
    Some(d * 1000 / r)
}

/// Gemini reads `text` aloud in `voice`; returns a WAV file.
pub fn speak(key: &str, text: &str, voice: &str) -> Result<Vec<u8>, AskError> {
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(30)).build();
    let mut model = TTS;
    loop {
        let url = format!("{API}/{model}:generateContent");
        match agent.post(&url).set("x-goog-api-key", key).send_json(tts_body(text, voice)) {
            Ok(r) => {
                let body = r.into_string().map_err(|_| AskError::Offline)?;
                return parse_tts(&body).ok_or_else(|| AskError::Other("no audio in reply".into()));
            }
            Err(e) => {
                let (c, b) = http_error(e);
                match c.and_then(|c| next_tts(model, c)) {
                    Some(next) => model = next,
                    None => return Err(classify(c, &b)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_maps_roles_and_tools() {
        let h = [Turn { user: true, text: "combo?".into() }, Turn { user: false, text: "Back, Forward + 1".into() }];
        let b = request_body(&h, true);
        assert_eq!(b["contents"][0]["role"], "user");
        assert_eq!(b["contents"][0]["parts"][0]["text"], "combo?");
        assert_eq!(b["contents"][1]["role"], "model");
        assert!(b["tools"][0].get("google_search").is_some());
        assert_eq!(b["system_instruction"]["parts"][0]["text"], SYSTEM);
        assert!(request_body(&h, false).get("tools").is_none(), "ungrounded retry has no tools");
    }

    #[test]
    fn sse_line_extracts_text() {
        let l = r#"data: {"candidates":[{"content":{"parts":[{"text":"Back, "},{"text":"Forward"}]}}]}"#;
        assert_eq!(parse_sse_line(l).as_deref(), Some("Back, Forward"));
        assert_eq!(parse_sse_line(""), None);
        assert_eq!(parse_sse_line(r#"data: {"usageMetadata":{}}"#), None);
    }

    #[test]
    fn classify_http_errors() {
        assert!(matches!(classify(None, ""), AskError::Offline));
        assert!(matches!(classify(Some(400), r#"{"error":{"message":"API key not valid. Please pass a valid API key."}}"#), AskError::BadKey));
        assert!(matches!(classify(Some(403), ""), AskError::BadKey));
        assert!(matches!(classify(Some(429), ""), AskError::Quota));
        assert!(matches!(classify(Some(404), "boom"), AskError::Other(_)));
        assert_eq!(AskError::Quota.message(), "Gemini's free limit is used up for now. Try again in a minute");
    }

    #[test]
    fn grounding_refusal_is_detected() {
        assert!(grounding_unsupported(400, r#"{"error":{"message":"Tool use with google_search is not supported"}}"#));
        assert!(!grounding_unsupported(400, "API key not valid"));
    }

    #[test]
    fn the_fast_model_answers_first() {
        // Measured 2026-11-03 on a free key: Flash-Lite answers in about 1 s; Flash was busy or hung.
        assert_eq!(MODEL, "gemini-flash-lite-latest");
        assert_eq!(FALLBACK, "gemini-flash-latest");
    }

    #[test]
    fn refused_search_retries_the_same_model_without_it() {
        let a = Attempt { model: MODEL, grounded: true };
        assert_eq!(next_attempt(a, 429, "RESOURCE_EXHAUSTED"), Some(Attempt { model: MODEL, grounded: false }));
        assert_eq!(next_attempt(a, 400, "google_search is not supported"), Some(Attempt { model: MODEL, grounded: false }));
    }

    #[test]
    fn busy_or_limited_model_falls_back() {
        let a = Attempt { model: MODEL, grounded: false };
        assert_eq!(next_attempt(a, 503, "UNAVAILABLE"), Some(Attempt { model: FALLBACK, grounded: false }));
        assert_eq!(next_attempt(a, 429, ""), Some(Attempt { model: FALLBACK, grounded: false }));
        let g = Attempt { model: MODEL, grounded: true };
        assert_eq!(next_attempt(g, 503, ""), Some(Attempt { model: FALLBACK, grounded: true }));
    }

    #[test]
    fn fallback_failures_and_real_errors_stop() {
        let l = Attempt { model: FALLBACK, grounded: false };
        assert_eq!(next_attempt(l, 429, ""), None);
        assert_eq!(next_attempt(l, 503, ""), None);
        assert_eq!(next_attempt(Attempt { model: MODEL, grounded: true }, 403, ""), None);
        assert_eq!(next_attempt(Attempt { model: MODEL, grounded: false }, 400, "API key not valid"), None);
    }

    #[test]
    fn a_hung_connection_is_busy_but_no_connection_is_offline() {
        assert_eq!(transport_status(ureq::ErrorKind::Io), Some(504));
        assert_eq!(transport_status(ureq::ErrorKind::ConnectionFailed), None);
        assert_eq!(transport_status(ureq::ErrorKind::Dns), None);
    }

    #[test]
    fn busy_has_its_own_message() {
        assert!(matches!(classify(Some(503), ""), AskError::Busy));
        assert_eq!(AskError::Busy.message(), "Gemini is busy right now. Try again in a moment");
    }

    #[test]
    fn tts_body_asks_for_audio_in_the_chosen_voice() {
        let b = tts_body("Back, Forward + 1", "Aoede");
        assert_eq!(b["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(b["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"], "Aoede");
        assert_eq!(b["contents"][0]["parts"][0]["text"], "Back, Forward + 1", "nothing else, or it is spoken too");
    }

    #[test]
    fn tts_reply_gives_wav_bytes() {
        use base64::Engine;
        let wav = b"RIFF....WAVEfmt ".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav);
        let body = format!(r#"{{"candidates":[{{"content":{{"parts":[{{"inlineData":{{"mimeType":"audio/wav","data":"{b64}"}}}}]}}}}]}}"#);
        assert_eq!(parse_tts(&body), Some(wav));
        // raw 24 kHz PCM is wrapped in a WAV header
        let pcm = base64::engine::general_purpose::STANDARD.encode([1u8, 0, 2, 0]);
        let body = format!(r#"{{"candidates":[{{"content":{{"parts":[{{"inlineData":{{"mimeType":"audio/L16;codec=pcm;rate=24000","data":"{pcm}"}}}}]}}}}]}}"#);
        let out = parse_tts(&body).unwrap();
        assert_eq!(&out[..4], b"RIFF");
        assert_eq!(&out[out.len() - 4..], &[1, 0, 2, 0]);
        assert_eq!(wav_duration_ms(&out), Some(0));
        assert_eq!(parse_tts("{}"), None);
    }

    #[test]
    fn wav_length_is_read_from_its_header() {
        let wav = pcm_to_wav(&vec![0u8; 48_000], 24_000); // 1 s of 16-bit mono at 24 kHz
        assert_eq!(wav_duration_ms(&wav), Some(1000));
        assert_eq!(wav_duration_ms(b"nope"), None);
    }

    #[test]
    fn busy_voice_model_falls_back_once() {
        assert_eq!(next_tts(TTS, 503), Some(TTS_FALLBACK));
        assert_eq!(next_tts(TTS, 429), Some(TTS_FALLBACK));
        assert_eq!(next_tts(TTS_FALLBACK, 503), None);
        assert_eq!(next_tts(TTS, 403), None);
    }
}
