//! `bridge_cli --bench`: measures how the pad really reports, over USB and over Bluetooth with each
//! Windows link preset. Reads the pad directly (no bridge, no virtual pad); results go to the console
//! and to bench.log next to the exe.

use pad_core::bridge::{DeviceInfo, Transport};
use pad_core::win::ble::{self, LinkPreset, LowLatency};
use pad_core::win::hid::{enumerate_stadia, now_us, HidDevice};
use std::io::Write;
use std::time::{Duration, Instant};

const MEASURE_S: u64 = 12;
/// Gaps longer than this are the user pausing the stick, not the link.
const IDLE_GAP_US: u64 = 100_000;

struct Log(std::fs::File);
impl Log {
    fn say(&mut self, s: &str) {
        println!("{s}");
        let _ = writeln!(self.0, "{s}");
    }
}

fn pct(v: &[u64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v[((v.len() - 1) as f64 * p).round() as usize] as f64 / 1000.0
}

fn measure(log: &mut Log, label: &str, dev: &DeviceInfo) {
    let Ok(d) = HidDevice::open_shared(&dev.path) else { return log.say(&format!("{label}: could not open the pad")) };
    let Ok(mut r) = d.reader(4) else { return log.say(&format!("{label}: could not start reading")) };
    println!("   >> {label}: move the RIGHT STICK in circles now (measuring {MEASURE_S} s once you start)...");
    // Wait for movement, then measure a fixed window.
    let mut prev = loop {
        match r.next(1000) {
            Ok(Some((_, t))) => break t,
            Ok(None) => continue,
            Err(e) => return log.say(&format!("{label}: read failed: {e}")),
        }
    };
    let end = prev + MEASURE_S * 1_000_000;
    let (mut gaps, mut idle, mut reports) = (Vec::new(), 0u32, 1u32);
    while now_us() < end {
        match r.next(200) {
            Ok(Some((_, t))) => {
                let g = t - prev;
                prev = t;
                reports += 1;
                if g > IDLE_GAP_US { idle += 1 } else { gaps.push(g) }
            }
            Ok(None) => {}
            Err(e) => return log.say(&format!("{label}: read failed: {e}")),
        }
    }
    gaps.sort_unstable();
    let active_s = gaps.iter().sum::<u64>() as f64 / 1e6;
    let bunched = gaps.iter().filter(|&&g| g < 1000).count() as f64 / gaps.len().max(1) as f64 * 100.0;
    let slow = gaps.iter().filter(|&&g| g > 12_000).count() as f64 / gaps.len().max(1) as f64 * 100.0;
    log.say(&format!(
        "{label:<28} reports {reports:>5}  rate {:>6.1} Hz  p50 {:>5.2} ms  p90 {:>5.2}  p99 {:>6.2}  max {:>6.2}  <1ms {bunched:>4.1}%  >12ms {slow:>4.1}%  pauses {idle}",
        gaps.len() as f64 / active_s.max(0.001),
        pct(&gaps, 0.5), pct(&gaps, 0.9), pct(&gaps, 0.99), pct(&gaps, 1.0)
    ));
}

fn wait_for(transport: Transport, secs: u64) -> Option<DeviceInfo> {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        let devs = enumerate_stadia();
        let usb = devs.iter().any(|d| d.transport == Transport::Usb);
        if let Some(d) = devs.into_iter().find(|d| d.transport == transport) {
            if transport == Transport::Usb || !usb {
                return Some(d);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

/// Other apps holding the pad (Pad Companion itself holds a Bluetooth link request) would skew results.
fn others_running() -> Vec<&'static str> {
    let out = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).output();
    let list = out.map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase()).unwrap_or_default();
    ["pad-companion.exe", "stadiapad.exe"].into_iter().filter(|n| list.contains(&format!("\"{n}\""))).collect()
}

pub fn run() {
    let path = std::env::current_exe().unwrap().with_file_name("bench.log");
    let mut log = Log(std::fs::File::create(&path).expect("bench.log"));
    loop {
        let others = others_running();
        if others.is_empty() {
            break;
        }
        println!("   >> Please quit {} (tray icon > Quit). Waiting...", others.join(" and "));
        std::thread::sleep(Duration::from_secs(2));
    }
    log.say("Pad Companion latency bench (no other pad app running).");
    log.say("Keep the right stick moving in circles during every step (pauses are ignored).\n");

    match wait_for(Transport::Usb, 3) {
        Some(d) => measure(&mut log, "USB cable", &d),
        None => log.say("No pad on the cable: skipping USB (plug it in and rerun to measure it)."),
    }

    println!("\n   >> Now UNPLUG the cable. Keep the pad switched on so it reconnects over Bluetooth...");
    let Some(d) = wait_for(Transport::Bluetooth, 180) else { return log.say("No Bluetooth pad within 3 min: stopping.") };
    // Give Windows time to drop back to its own link after any request another app just released.
    std::thread::sleep(Duration::from_secs(6));
    let addr = ble::bt_addr_from_path(&d.path);
    let presets: [(&str, Option<LinkPreset>); 4] = [
        ("Bluetooth, Windows default", None),
        ("Bluetooth, Balanced", Some(LinkPreset::Balanced)),
        ("Bluetooth, ThroughputOptimized", Some(LinkPreset::Throughput)),
        ("Bluetooth, PowerOptimized", Some(LinkPreset::Power)),
    ];
    for (label, preset) in presets {
        let guard = match (preset, addr) {
            (Some(p), Some(a)) => match LowLatency::request_preset(a, p) {
                Ok(g) => Some(g),
                Err(s) => {
                    log.say(&format!("{label}: request refused ({s:?})"));
                    continue;
                }
            },
            _ => None,
        };
        std::thread::sleep(Duration::from_secs(2)); // let the link renegotiate
        if let Some(lp) = addr.and_then(ble::link_params) {
            log.say(&format!("   link now: interval {:.2} ms, latency {} events, timeout {} ms", lp.interval_ms, lp.latency, lp.timeout_ms));
        }
        measure(&mut log, label, &d);
        if let Some(lp) = addr.and_then(ble::link_params) {
            log.say(&format!("   link after: interval {:.2} ms", lp.interval_ms));
        }
        drop(guard);
        std::thread::sleep(Duration::from_secs(2));
    }
    log.say(&format!("\nDone. Results saved to {}", path.display()));
}
