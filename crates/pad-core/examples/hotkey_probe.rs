//! Manual check of the overlay shortcut: registers Ctrl+Alt+G, then press/hold it within the time given.
//! Run: cargo run -p pad-core --example hotkey_probe -- [seconds]
use pad_core::hotkey::{label, Hotkey};
use pad_core::win::hotkey_win::HotkeyThread;

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let t = HotkeyThread::start(|a| println!("  action {a:?}"));
    let k = Hotkey::default();
    println!("register {}: {:?}", label(&k), t.set(Some(k)));
    println!("plain G: {:?}", t.set(Some(Hotkey { ctrl: false, alt: false, shift: false, win: false, vk: 0x47 })));
    std::thread::sleep(std::time::Duration::from_secs(secs));
    println!("off: {:?}", t.set(None));
}
