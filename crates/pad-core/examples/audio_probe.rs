//! Manual check of Live audio: records 3 s from the default mic (16 kHz mono), prints its loudest
//! moment, then plays it back on the speakers. `--no-play` only records.
//! Run: cargo run -p pad-core --example audio_probe [-- --no-play]
use pad_core::live::level;
use pad_core::win::audio::{Mic, Player};
use std::sync::{Arc, Mutex};

fn main() {
    let got: Arc<Mutex<Vec<i16>>> = Arc::default();
    let g = got.clone();
    let mic = Mic::start(move |chunk| g.lock().unwrap().extend(chunk)).expect("mic");
    std::thread::sleep(std::time::Duration::from_secs(3));
    drop(mic);
    let pcm = got.lock().unwrap().clone();
    let peak = pcm.chunks(1600).map(level).fold(0.0f32, f32::max);
    println!("recorded {} samples ({} ms), peak level {peak:.3}", pcm.len(), pcm.len() / 16);
    if std::env::args().any(|a| a == "--no-play") {
        return;
    }
    // 16 kHz -> 24 kHz for the player (it expects Gemini's rate).
    let up: Vec<i16> = (0..pcm.len() * 3 / 2).map(|i| pcm[(i * 2 / 3).min(pcm.len() - 1)]).collect();
    let player = Player::start().expect("speakers");
    player.push(&up);
    while player.queued_ms() > 0 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!("played back");
}
