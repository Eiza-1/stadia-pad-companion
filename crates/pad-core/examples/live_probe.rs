//! Manual check of Gemini Live with the key saved in Pad Companion.
//!   --text "what is a hadouken"   one typed turn (mic muted), prints captions and timing
//!   --talk 8                      open mic for 8 s (Speakers mode)
//!   --no-play                     don't play Gemini's voice
//! Run: cargo run -p pad-core --example live_probe -- --text "what is a hadouken" --no-play
use pad_core::settings::LiveListen;
use pad_core::win::keystore;
use pad_core::win::live_session::{LiveConfig, LiveSession, LiveUpdate};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let key = keystore::get().expect("No Gemini key saved yet");
    let play = !args.iter().any(|a| a == "--no-play");
    let (tx, rx) = mpsc::channel();
    let t0 = Instant::now();
    let s = LiveSession::start(LiveConfig { key, voice: "Aoede".into(), listen: LiveListen::Speakers, monitor_of: 0, play }, move |u| {
        let _ = tx.send(u);
    });
    let text = arg("--text");
    let talk = arg("--talk").and_then(|t| t.parse::<u64>().ok());
    s.set_muted(text.is_some());
    let deadline = Instant::now() + Duration::from_secs(talk.unwrap_or(20) + 5);
    let mut said = String::new();
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(LiveUpdate::Ready) => {
                println!("ready after {} ms", t0.elapsed().as_millis());
                if let Some(t) = &text {
                    s.send_text(t);
                }
            }
            Ok(LiveUpdate::GeminiSaid(t)) => {
                if said.is_empty() {
                    println!("first words after {} ms", t0.elapsed().as_millis());
                }
                said.push_str(&t);
            }
            Ok(LiveUpdate::YouSaid(t)) => println!("you: {t}"),
            Ok(LiveUpdate::TurnDone) if text.is_some() => break,
            Ok(LiveUpdate::Ended(why)) => {
                println!("ended: {why:?}");
                break;
            }
            _ => {}
        }
    }
    println!("gemini: {said}");
    s.end();
    println!("closed");
}
