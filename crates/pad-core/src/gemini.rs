//! Gemini API client for the overlay: builds requests, streams answers, turns failures into one readable
//! line. The player's own key travels only in the `x-goog-api-key` header.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// The model ids live only here. `LITE` is the fallback when Flash is busy or its free limit is hit.
pub const MODEL: &str = "gemini-flash-latest";
pub const LITE: &str = "gemini-flash-lite-latest";
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
/// grounding first; a busy or rate-limited Flash then falls back to Flash-Lite.
pub fn next_attempt(a: Attempt, status: u16, body: &str) -> Option<Attempt> {
    if a.grounded && (status == 429 || grounding_unsupported(status, body)) {
        return Some(Attempt { grounded: false, ..a });
    }
    if a.model == MODEL && matches!(status, 429 | 500 | 502 | 503 | 504) {
        return Some(Attempt { model: LITE, ..a });
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
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(20)).build()
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
    fn refused_search_retries_the_same_model_without_it() {
        let a = Attempt { model: MODEL, grounded: true };
        assert_eq!(next_attempt(a, 429, "RESOURCE_EXHAUSTED"), Some(Attempt { model: MODEL, grounded: false }));
        assert_eq!(next_attempt(a, 400, "google_search is not supported"), Some(Attempt { model: MODEL, grounded: false }));
    }

    #[test]
    fn busy_or_limited_flash_falls_back_to_lite() {
        let a = Attempt { model: MODEL, grounded: false };
        assert_eq!(next_attempt(a, 503, "UNAVAILABLE"), Some(Attempt { model: LITE, grounded: false }));
        assert_eq!(next_attempt(a, 429, ""), Some(Attempt { model: LITE, grounded: false }));
        let g = Attempt { model: MODEL, grounded: true };
        assert_eq!(next_attempt(g, 503, ""), Some(Attempt { model: LITE, grounded: true }));
    }

    #[test]
    fn lite_failures_and_real_errors_stop() {
        let l = Attempt { model: LITE, grounded: false };
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
}
