//! Manual check of live dictation: speak for a few seconds after "listening".
//! Run: cargo run -p pad-core --example dictation_probe -- [seconds]
use pad_core::win::speech::{Dictation, SpeechEvent};

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    println!("listening for {secs} s…");
    let d = Dictation::start(|e| match e {
        SpeechEvent::Partial(t) => println!("  … {t}"),
        SpeechEvent::Final(t) => println!("  ✓ {t}"),
        SpeechEvent::Error(m) => println!("  ! {m}"),
    });
    std::thread::sleep(std::time::Duration::from_secs(secs));
    println!("transcript: {:?}", d.stop());
}
