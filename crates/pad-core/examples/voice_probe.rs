//! Manual check of Gemini's read-aloud voice with the key saved in Pad Companion.
//! Run: cargo run -p pad-core --example voice_probe -- Aoede "A Hadouken is a fireball special move."
use pad_core::gemini::{speak, wav_duration_ms};
use pad_core::win::{keystore, sound};
use std::time::Instant;

fn main() {
    let key = keystore::get().expect("No Gemini key saved yet (Pad Companion › Other settings › Gemini).");
    let mut args = std::env::args().skip(1);
    let voice = args.next().unwrap_or_else(|| "Aoede".into());
    let text = args.next().unwrap_or_else(|| "A Hadouken is a fireball special move: down, down-forward, forward, then punch.".into());
    let t = Instant::now();
    match speak(&key, &text, &voice) {
        Ok(wav) => {
            let ms = wav_duration_ms(&wav).unwrap_or(0);
            println!("{voice}: {} KB, {ms} ms of speech, ready in {} ms", wav.len() / 1024, t.elapsed().as_millis());
            println!("playing: {}", sound::play(wav));
            std::thread::sleep(std::time::Duration::from_millis(ms + 300));
            sound::stop();
        }
        Err(e) => eprintln!("{} ({e:?})", e.message()),
    }
}
