//! Gemini API client for the overlay: builds requests, streams answers, turns failures into one readable
//! line. The player's own key travels only in the `x-goog-api-key` header.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// The only place the model id lives.
pub const MODEL: &str = "gemini-flash-latest";
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
    Other(String),
}

impl AskError {
    /// One line for the answer card.
    pub fn message(&self) -> &'static str {
        match self {
            AskError::Offline => "No internet connection",
            AskError::BadKey => "Your Gemini key isn't working. Check it in Settings",
            AskError::Quota => "Gemini's free limit is used up for now. Try again in a minute",
            AskError::Other(_) => "Gemini couldn't answer",
        }
    }
}

/// `status` is `None` when no HTTP response came back at all.
pub fn classify(status: Option<u16>, body: &str) -> AskError {
    match status {
        None => AskError::Offline,
        Some(429) => AskError::Quota,
        Some(401 | 403) => AskError::BadKey,
        Some(400) if body.contains("API key not valid") || body.contains("API_KEY_INVALID") => AskError::BadKey,
        Some(code) => AskError::Other(format!("HTTP {code}: {}", body.chars().take(200).collect::<String>())),
    }
}

/// A 400 that says the key's tier can't use Google Search grounding: retry without it.
pub fn grounding_unsupported(status: u16, body: &str) -> bool {
    status == 400 && body.contains("google_search")
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
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(30)).build()
}

fn http_error(e: ureq::Error) -> (Option<u16>, String) {
    match e {
        ureq::Error::Status(code, r) => (Some(code), r.into_string().unwrap_or_default()),
        ureq::Error::Transport(_) => (None, String::new()),
    }
}

/// Streams Gemini's answer to the conversation, calling `on_delta` with each piece; returns the full text.
pub fn ask_stream(key: &str, history: &[Turn], mut on_delta: impl FnMut(&str)) -> Result<String, AskError> {
    let url = format!("{API}/{MODEL}:streamGenerateContent?alt=sse");
    let send = |grounded: bool| {
        agent().post(&url).set("x-goog-api-key", key).send_json(request_body(history, grounded))
    };
    let resp = match send(true) {
        Ok(r) => r,
        Err(e) => match http_error(e) {
            (Some(code), body) if grounding_unsupported(code, &body) => {
                send(false).map_err(|e| { let (c, b) = http_error(e); classify(c, &b) })?
            }
            (c, b) => return Err(classify(c, &b)),
        },
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
        assert!(matches!(classify(Some(500), "boom"), AskError::Other(_)));
        assert_eq!(AskError::Quota.message(), "Gemini's free limit is used up for now. Try again in a minute");
    }

    #[test]
    fn grounding_refusal_is_detected() {
        assert!(grounding_unsupported(400, r#"{"error":{"message":"Tool use with google_search is not supported"}}"#));
        assert!(!grounding_unsupported(400, "API key not valid"));
    }
}
