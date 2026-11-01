//! `bridge_cli --sticks`: flick both sticks and let go, repeatedly, for 20 s, with the real bridge
//! running exactly as the app runs it (live UI events on). At every rest it checks:
//! - where the stick really rests (raw reports read alongside the bridge),
//! - what games receive there (the user's profile),
//! - whether the UI was sent that final position, and how long after the report.
//!
//! A rest is the last report before the pad goes quiet for 100 ms (it stops sending once the stick
//! is still). UI events are collected every 2 ms, so the measured UI delay is accurate to ~2 ms.
//! Writes sticks.log (summary) and sticks-trace.csv (every raw report and UI event) next to the exe.

use pad_core::bridge::{Bridge, BridgeEvent};
use pad_core::mapping::{Mapper, XOut};
use pad_core::report::{parse_input, PadState};
use pad_core::settings::Settings;
use pad_core::win::hid::{enumerate_stadia, now_us, HidDevice};
use std::sync::mpsc;
use std::time::Duration;

const QUIET_US: u64 = 100_000;

fn pct(x: u8, y: u8) -> f32 {
    let fx = (x as f32 - 127.5) / 127.5;
    let fy = (y as f32 - 127.5) / 127.5;
    (fx * fx + fy * fy).sqrt() * 100.0
}

fn sticks(s: &PadState) -> [u8; 4] {
    [s.lx, s.ly, s.rx, s.ry]
}

#[derive(Default)]
struct Summary {
    rests: u32,
    worst_rest_pct: f32,
    game_nonzero: u32,
    ui_missed: u32,
    ui_delay_ms: Vec<f32>,
}

pub fn run() {
    let settings = Settings::load(&Settings::default_path());
    let profile = settings.active().clone();
    let (mode, stadia) = (settings.pad_mode, settings.stadia);
    let dz = (profile.left.deadzone_pct, profile.right.deadzone_pct);
    let bridge = Bridge::start(settings);
    bridge.set_live(true);
    let (tx, rx) = mpsc::channel();
    bridge.subscribe(tx);

    let Some(d) = (0..20).find_map(|_| {
        std::thread::sleep(Duration::from_millis(250));
        enumerate_stadia().into_iter().next()
    }) else {
        return println!("no pad");
    };
    let dev = HidDevice::open_shared(&d.path).expect("open");
    let mut r = dev.reader(4).expect("reader");
    let mut mapper = Mapper::new(profile, mode, stadia);
    println!("{:?}, {mode:?}, dead zones {}% / {}%", d.transport, dz.0, dz.1);
    println!("   >> Flick BOTH sticks hard and let go, again and again. Starts at your first flick, runs 20 s.");

    let mut trace = String::from("kind,t_us,data\n");
    // Latest UI state and when it arrived.
    let mut ui: Option<(u64, [u8; 4])> = None;
    let drain = |ui: &mut Option<(u64, [u8; 4])>, trace: &mut String| {
        while let Ok(ev) = rx.try_recv() {
            if let BridgeEvent::Pad { state, .. } = ev {
                let (t, v) = (now_us(), sticks(&state));
                *ui = Some((t, v));
                trace.push_str(&format!("ui,{t},{} {} {} {}\n", v[0], v[1], v[2], v[3]));
            }
        }
    };
    loop {
        drain(&mut ui, &mut trace);
        if let Ok(Some((data, _))) = r.next(200) {
            if parse_input(&data).is_some_and(|s| pct(s.lx, s.ly) > 40.0 || pct(s.rx, s.ry) > 40.0) {
                break;
            }
        }
    }
    println!("   measuring 20 s...");
    let mut sum = Summary::default();
    let mut last: Option<(u64, PadState, XOut)> = None;
    let mut settled = true;
    let mut max_gap = 0u64;
    let end = now_us() + 20_000_000;
    while now_us() < end {
        match r.next(2) {
            Ok(Some((data, t))) => {
                if let Some((pt, ..)) = last {
                    max_gap = max_gap.max(t - pt);
                }
                let hex: Vec<String> = data.iter().map(|b| format!("{b:02x}")).collect();
                trace.push_str(&format!("raw,{t},{}\n", hex.join(" ")));
                if let Some(s) = parse_input(&data) {
                    last = Some((t, s, mapper.map(&s)));
                    settled = false;
                }
                drain(&mut ui, &mut trace);
            }
            _ => {
                drain(&mut ui, &mut trace);
                let Some((t, s, o)) = last else { continue };
                if settled || now_us() - t < QUIET_US {
                    continue;
                }
                // The pad went quiet: `s` is where the sticks really rest.
                settled = true;
                sum.rests += 1;
                trace.push_str(&format!("rest,{t},{} {} {} {}\n", s.lx, s.ly, s.rx, s.ry));
                sum.worst_rest_pct = sum.worst_rest_pct.max(pct(s.lx, s.ly)).max(pct(s.rx, s.ry));
                if (o.lx, o.ly, o.rx, o.ry) != (0, 0, 0, 0) {
                    sum.game_nonzero += 1;
                }
                match ui {
                    Some((ut, v)) if v == sticks(&s) => sum.ui_delay_ms.push(ut.saturating_sub(t) as f32 / 1000.0),
                    _ => sum.ui_missed += 1,
                }
            }
        }
    }
    drop(r);
    drop(dev);
    bridge.stop();

    sum.ui_delay_ms.sort_by(f32::total_cmp);
    let median = sum.ui_delay_ms.get(sum.ui_delay_ms.len() / 2).copied().unwrap_or(0.0);
    let worst = sum.ui_delay_ms.last().copied().unwrap_or(0.0);
    let text = format!(
        "{:?}, {mode:?}, dead zones {}% / {}%, longest gap between reports {:.1} ms\n\
         rests {}  worst resting offset {:.1}%  games non-zero at rest {}\n\
         UI missed the resting position {}  UI delay after the final report: median {median:.1} ms, worst {worst:.1} ms\n",
        d.transport, dz.0, dz.1, max_gap as f32 / 1000.0, sum.rests, sum.worst_rest_pct, sum.game_nonzero, sum.ui_missed
    );
    print!("{text}");
    let exe = std::env::current_exe().unwrap();
    let _ = std::fs::write(exe.with_file_name("sticks.log"), text);
    let _ = std::fs::write(exe.with_file_name("sticks-trace.csv"), trace);
}
