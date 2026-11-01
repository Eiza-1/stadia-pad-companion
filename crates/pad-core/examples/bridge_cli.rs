//! Headless bridge for hardware checks: prints status changes, stats every second and actions.
//! Press Ctrl+C or Enter to stop (the pad is un-hidden on exit).

use pad_core::bridge::{Bridge, BridgeEvent};
use pad_core::settings::{PadMode, Settings};
use std::sync::mpsc;

fn main() {
    if std::env::args().any(|a| a == "--info") {
        for d in pad_core::win::hid::enumerate_stadia() {
            println!("{:?} {} {:?}", d.transport, d.path, pad_core::win::hid::read_device_info(&d.path));
        }
        return;
    }
    let mut settings = Settings::load(&Settings::default_path());
    if std::env::args().any(|a| a == "--competitive") {
        settings.pad_mode = PadMode::Competitive;
    }
    println!("mode {:?}", settings.pad_mode);
    let bridge = Bridge::start(settings);
    let (tx, rx) = mpsc::channel();
    bridge.subscribe(tx.clone());

    let (quit_tx, quit_rx) = mpsc::channel::<()>();
    let q = quit_tx.clone();
    ctrlc::set_handler(move || {
        let _ = q.send(());
    })
    .expect("ctrl-c handler");
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        let _ = quit_tx.send(());
    });

    let mut last_pad = None;
    loop {
        if quit_rx.try_recv().is_ok() {
            break;
        }
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(BridgeEvent::Status(s)) => println!("status {}", serde_json::to_string(&s).unwrap()),
            Ok(BridgeEvent::Stats(s)) => {
                let pad = last_pad.take().map(|(st, o): (pad_core::report::PadState, pad_core::mapping::XOut)| {
                    format!(" | buttons={:?} lx={} ly={} lt={}", st.buttons, o.lx, o.ly, o.lt)
                });
                println!(
                    "stats rate={:.1}Hz p50={:.2}ms p99={:.2}ms max={:.2}ms proc_p99={}us n={}{}",
                    s.rate_hz, s.interval_p50_ms, s.interval_p99_ms, s.interval_max_ms, s.proc_p99_us, s.samples,
                    pad.unwrap_or_default()
                )
            }
            Ok(BridgeEvent::Pad { state, out }) => last_pad = Some((state, out)),
            Ok(BridgeEvent::Action(a)) => println!("action {a:?}"),
            Err(_) => {}
        }
    }
    println!("stopping…");
    bridge.stop();
    println!("stopped");
}
