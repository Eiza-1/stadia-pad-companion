//! Manual check of the Gemini client with the key saved in Pad Companion (Credential Manager).
//! Run: cargo run -p pad-core --example gemini_probe -- "what is a hadouken?"
use pad_core::gemini::{ask_stream, validate_key, Turn};
use pad_core::win::keystore;
use std::io::Write;

fn main() {
    let Some(key) = keystore::get() else {
        eprintln!("No Gemini key saved yet (Pad Companion › Other settings › Gemini).");
        std::process::exit(2);
    };
    if let Err(e) = validate_key(&key) {
        eprintln!("key check: {} ({e:?})", e.message());
        std::process::exit(1);
    }
    let q = std::env::args().nth(1).unwrap_or_else(|| "what is a hadouken?".into());
    let r = ask_stream(&key, &[Turn { user: true, text: q }], |d| {
        print!("{d}");
        let _ = std::io::stdout().flush();
    });
    println!();
    if let Err(e) = r {
        eprintln!("{} ({e:?})", e.message());
    }
}
