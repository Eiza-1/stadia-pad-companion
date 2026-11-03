//! Gemini Live: the messages of the Live API (a WebSocket, `BidiGenerateContent`), audio conversion for
//! the mic (16 kHz) and Gemini's voice (24 kHz), and the rules for when the mic is open and when a
//! screen frame goes out. Pure; `win::live_session` does the I/O.

use crate::settings::LiveListen;
use serde_json::{json, Value};

/// Live model ids live only here (both verified on a free key 2026-11-03; the first was fastest).
pub const LIVE_MODEL: &str = "gemini-3.1-flash-live-preview";
pub const LIVE_FALLBACK: &str = "gemini-3.8-live";
pub const MIC_RATE: u32 = 16_000;
pub const OUT_RATE: u32 = 24_000;
/// Speakers mode keeps the mic closed this long after Gemini's last audio (the room's echo).
pub const ECHO_TAIL_MS: u64 = 300;
pub const FRAME_EVERY_MS: u64 = 1000;

pub const LIVE_SYSTEM: &str = "You are a friendly gaming assistant talking with a player while they play. Speak naturally and briefly: one to three short sentences unless they ask for more. Never read symbols, lists or button notation literally; say them the way a person would. If they share their screen, answer about what is on it now. Answer in the language the player speaks.";

pub fn ws_url(key: &str) -> String {
    format!("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={key}")
}

pub fn setup_msg(model: &str, voice: &str, grounded: bool, resume: Option<&str>) -> Value {
    let mut setup = json!({
        "model": format!("models/{model}"),
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
        },
        "systemInstruction": { "parts": [{ "text": LIVE_SYSTEM }] },
        "inputAudioTranscription": {},
        "outputAudioTranscription": {},
        // Keeps a session going past the free tier's short limits, and lets it resume after a goAway.
        "contextWindowCompression": { "slidingWindow": {} },
        "sessionResumption": {}
    });
    if let Some(h) = resume {
        setup["sessionResumption"]["handle"] = json!(h);
    }
    if grounded {
        setup["tools"] = json!([{ "google_search": {} }]);
    }
    json!({ "setup": setup })
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A chunk of mic audio (16 kHz mono).
pub fn audio_msg(pcm: &[i16]) -> Value {
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    json!({ "realtimeInput": { "audio": { "data": b64(&bytes), "mimeType": "audio/pcm;rate=16000" } } })
}

/// One shared-screen picture.
pub fn frame_msg(jpeg: &[u8]) -> Value {
    json!({ "realtimeInput": { "video": { "data": b64(jpeg), "mimeType": "image/jpeg" } } })
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveEvent {
    SetupComplete,
    /// Gemini's voice, 24 kHz mono.
    Audio(Vec<i16>),
    /// Transcript of the player's speech (pieces).
    YouSaid(String),
    /// Transcript of Gemini's speech (pieces).
    GeminiSaid(String),
    /// The player talked over Gemini: drop what is still queued to play.
    Interrupted,
    TurnComplete,
    /// Google will close the connection soon: reconnect with the resumption handle.
    GoAway,
    Resume(String),
    Error(String),
}

/// Everything one server message carries, in order: audio, Gemini's words, the player's words, flags.
pub fn parse_server(text: &str) -> Vec<LiveEvent> {
    use base64::Engine;
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let mut out = vec![];
    if v.get("setupComplete").is_some() {
        out.push(LiveEvent::SetupComplete);
    }
    let sc = &v["serverContent"];
    for p in sc["modelTurn"]["parts"].as_array().into_iter().flatten() {
        if let Some(d) = p["inlineData"]["data"].as_str() {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(d) {
                out.push(LiveEvent::Audio(bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()));
            }
        }
    }
    if let Some(t) = sc["outputTranscription"]["text"].as_str().filter(|t| !t.is_empty()) {
        out.push(LiveEvent::GeminiSaid(t.to_string()));
    }
    if let Some(t) = sc["inputTranscription"]["text"].as_str().filter(|t| !t.is_empty()) {
        out.push(LiveEvent::YouSaid(t.to_string()));
    }
    if sc["interrupted"].as_bool() == Some(true) {
        out.push(LiveEvent::Interrupted);
    }
    if sc["turnComplete"].as_bool() == Some(true) {
        out.push(LiveEvent::TurnComplete);
    }
    if v.get("goAway").is_some() {
        out.push(LiveEvent::GoAway);
    }
    if let Some(h) = v["sessionResumptionUpdate"]["newHandle"].as_str() {
        out.push(LiveEvent::Resume(h.to_string()));
    }
    if let Some(m) = v["error"]["message"].as_str() {
        out.push(LiveEvent::Error(m.to_string()));
    }
    out
}

/// Linear resampling of mono samples from `from` Hz to `to` Hz.
fn resample(mono: &[f32], from: u32, to: u32) -> Vec<f32> {
    if mono.is_empty() || from == 0 {
        return vec![];
    }
    let n = (mono.len() as u64 * to as u64 / from as u64) as usize;
    let step = from as f64 / to as f64;
    (0..n)
        .map(|i| {
            let pos = i as f64 * step;
            let k = pos as usize;
            let f = (pos - k as f64) as f32;
            let a = mono[k.min(mono.len() - 1)];
            let b = mono[(k + 1).min(mono.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

/// Mic samples (any channels and rate, -1..1) to what Live wants: 16 kHz mono 16-bit.
pub fn to_mono_16k(samples: &[f32], channels: u16, rate: u32) -> Vec<i16> {
    let ch = channels.max(1) as usize;
    let mono: Vec<f32> = samples.chunks(ch).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect();
    resample(&mono, rate, MIC_RATE).into_iter().map(|x| (x.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16).collect()
}

/// Gemini's 24 kHz voice to the output device's rate, as -1..1 samples.
pub fn resample_24k_to(pcm: &[i16], rate: u32) -> Vec<f32> {
    let mono: Vec<f32> = pcm.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
    resample(&mono, OUT_RATE, rate)
}

/// Loudness 0..1 for the orb (normal speech is about 0.3 to 0.8).
pub fn level(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let rms = (pcm.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt();
    (rms / 8000.0).min(1.0) as f32
}

/// Is the mic sent to Gemini right now? Speakers: not while Gemini's voice is playing (or for the echo
/// tail after it), unless the player holds Assistant to talk over it. Headphones: always, unless muted.
pub fn mic_open(listen: LiveListen, muted: bool, talk_over: bool, last_audio_ms: Option<u64>, now_ms: u64) -> bool {
    if muted {
        return false;
    }
    match listen {
        LiveListen::Headphones => true,
        LiveListen::Speakers => talk_over || last_audio_ms.is_none_or(|t| now_ms >= t + ECHO_TAIL_MS),
    }
}

/// Is a shared-screen frame due? Never while Share screen is off.
pub fn frame_due(sharing: bool, last_ms: Option<u64>, now_ms: u64) -> bool {
    sharing && last_ms.is_none_or(|t| now_ms >= t + FRAME_EVERY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

    #[test]
    fn setup_has_model_voice_transcripts_and_resumption() {
        let m = setup_msg(LIVE_MODEL, "Aoede", true, Some("h1"));
        let s = &m["setup"];
        assert_eq!(s["model"], "models/gemini-3.1-flash-live-preview");
        assert_eq!(s["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(s["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"], "Aoede");
        assert_eq!(s["systemInstruction"]["parts"][0]["text"], LIVE_SYSTEM);
        assert!(s["inputAudioTranscription"].is_object());
        assert!(s["outputAudioTranscription"].is_object());
        assert!(s["contextWindowCompression"]["slidingWindow"].is_object());
        assert_eq!(s["sessionResumption"]["handle"], "h1");
        assert!(s["tools"][0]["google_search"].is_object());
        let plain = setup_msg(LIVE_MODEL, "Kore", false, None);
        assert!(plain["setup"].get("tools").is_none());
        assert!(plain["setup"]["sessionResumption"].is_object());
        assert!(plain["setup"]["sessionResumption"].get("handle").is_none());
    }

    #[test]
    fn audio_msg_is_little_endian_base64() {
        let m = audio_msg(&[1, -2]);
        assert_eq!(m["realtimeInput"]["audio"]["mimeType"], "audio/pcm;rate=16000");
        let bytes = B64.decode(m["realtimeInput"]["audio"]["data"].as_str().unwrap()).unwrap();
        assert_eq!(bytes, vec![0x01, 0x00, 0xfe, 0xff]);
        let f = frame_msg(&[0xff, 0xd8]);
        assert_eq!(f["realtimeInput"]["video"]["mimeType"], "image/jpeg");
        assert_eq!(B64.decode(f["realtimeInput"]["video"]["data"].as_str().unwrap()).unwrap(), vec![0xff, 0xd8]);
    }

    #[test]
    fn parse_audio_and_captions() {
        let pcm = B64.encode([1u8, 0, 2, 0]);
        let msg = format!(
            r#"{{"serverContent":{{"modelTurn":{{"parts":[{{"inlineData":{{"mimeType":"audio/pcm;rate=24000","data":"{pcm}"}}}}]}},"outputTranscription":{{"text":"Hi"}},"inputTranscription":{{"text":"where"}}}}}}"#
        );
        assert_eq!(parse_server(&msg), vec![LiveEvent::Audio(vec![1, 2]), LiveEvent::GeminiSaid("Hi".into()), LiveEvent::YouSaid("where".into())]);
    }

    #[test]
    fn parse_flags() {
        assert_eq!(parse_server(r#"{"serverContent":{"interrupted":true}}"#), vec![LiveEvent::Interrupted]);
        assert_eq!(parse_server(r#"{"serverContent":{"turnComplete":true}}"#), vec![LiveEvent::TurnComplete]);
        assert_eq!(parse_server(r#"{"setupComplete":{}}"#), vec![LiveEvent::SetupComplete]);
        assert_eq!(parse_server(r#"{"goAway":{"timeLeft":"5s"}}"#), vec![LiveEvent::GoAway]);
        assert_eq!(parse_server(r#"{"sessionResumptionUpdate":{"newHandle":"h2","resumable":true}}"#), vec![LiveEvent::Resume("h2".into())]);
        assert_eq!(parse_server(r#"{"error":{"message":"quota"}}"#), vec![LiveEvent::Error("quota".into())]);
        assert_eq!(parse_server("not json"), vec![]);
    }

    #[test]
    fn stereo_48k_becomes_mono_16k() {
        let s = vec![0.5f32; 4800 * 2];
        let out = to_mono_16k(&s, 2, 48_000);
        assert_eq!(out.len(), 1600);
        assert!(out.iter().all(|&x| (16_380..=16_385).contains(&x)), "{}", out[0]);
        assert_eq!(to_mono_16k(&[2.0, -2.0], 1, 16_000), vec![i16::MAX, -i16::MAX], "clamped");
        let up = resample_24k_to(&[0, 16384], 48_000);
        assert_eq!(up.len(), 4);
        assert!((up[0]).abs() < 1e-6);
    }

    #[test]
    fn level_of_silence_is_zero_and_loud_is_one() {
        assert_eq!(level(&[0; 160]), 0.0);
        assert_eq!(level(&[i16::MAX; 160]), 1.0);
        assert_eq!(level(&[]), 0.0);
        let speech = level(&[4000; 160]);
        assert!(speech > 0.3 && speech < 0.8, "{speech}");
    }

    #[test]
    fn mic_gate_closes_while_gemini_speaks() {
        use LiveListen::*;
        assert!(!mic_open(Speakers, false, false, Some(1000), 1000));
        assert!(!mic_open(Speakers, false, false, Some(1000), 1299));
        assert!(mic_open(Speakers, false, false, Some(1000), 1300));
        assert!(mic_open(Speakers, false, false, None, 5));
        assert!(mic_open(Speakers, false, true, Some(1000), 1000), "holding Assistant talks over Gemini");
        assert!(!mic_open(Speakers, true, true, None, 5), "muted is muted");
        assert!(mic_open(Headphones, false, false, Some(1000), 1000), "open mic on headphones");
        assert!(!mic_open(Headphones, true, false, None, 0));
    }

    #[test]
    fn no_frames_while_share_is_off() {
        for (last, now) in [(None, 0), (Some(0), 5000), (Some(10), 10)] {
            assert!(!frame_due(false, last, now));
        }
        assert!(frame_due(true, None, 0));
        assert!(!frame_due(true, Some(0), 999));
        assert!(frame_due(true, Some(0), 1000));
    }
}
