//! `bridge_cli --triggers`: 15 s of fully pressing L2/R2. Reports each trigger's raw peak, the raw value
//! whenever its bottom-out "click" bit is set, and what games would receive (default profile).

use pad_core::mapping::{Mapper, PadMode, Profile, StadiaAction};
use pad_core::report::{parse_input, Buttons};
use pad_core::win::hid::{enumerate_stadia, now_us, HidDevice};

#[derive(Default)]
struct T {
    peak: u8,
    click_raw_min: Option<u8>,
    clicked: u32,
    click_not_full_out: u32,
    out_peak: u8,
    presses: u32,
    was_click: bool,
}

pub fn run() {
    let Some(d) = enumerate_stadia().into_iter().next() else { return println!("no pad") };
    let dev = HidDevice::open_shared(&d.path).expect("open");
    let mut r = dev.reader(4).expect("reader");
    let mut mapper = Mapper::new(Profile::default(), PadMode::Basic, StadiaAction::GuideHold);
    println!("{:?}: press L2 and R2 ALL the way down, release, repeat. Starts at your first press, runs 15 s.", d.transport);
    let (mut l, mut rt) = (T::default(), T::default());
    // The window opens before the user is ready: start the 15 s at the first trigger movement.
    loop {
        if let Ok(Some((data, _))) = r.next(500) {
            if parse_input(&data).is_some_and(|s| s.l2 > 20 || s.r2 > 20) {
                break;
            }
        }
    }
    println!("   measuring 15 s...");
    let end = now_us() + 15_000_000;
    while now_us() < end {
        let Ok(Some((data, _))) = r.next(200) else { continue };
        let Some(s) = parse_input(&data) else { continue };
        let o = mapper.map(&s);
        for (t, raw, out, flag) in [(&mut l, s.l2, o.lt, Buttons::L2_CLICK), (&mut rt, s.r2, o.rt, Buttons::R2_CLICK)] {
            t.peak = t.peak.max(raw);
            t.out_peak = t.out_peak.max(out);
            let click = s.buttons.contains(flag);
            if click {
                t.clicked += 1;
                t.click_raw_min = Some(t.click_raw_min.map_or(raw, |m| m.min(raw)));
                if out < 255 {
                    t.click_not_full_out += 1;
                }
            }
            if click && !t.was_click {
                t.presses += 1;
            }
            t.was_click = click;
        }
    }
    let mut text = format!("{:?}
", d.transport);
    for (name, t) in [("L2", &l), ("R2", &rt)] {
        text += &format!(
            "{name}: full presses {}  raw peak {}  raw while clicked: min {:?}  game value peak {}  reports clicked {} of which game < 255: {}
",
            t.presses, t.peak, t.click_raw_min, t.out_peak, t.clicked, t.click_not_full_out
        );
    }
    print!("{text}");
    let _ = std::fs::write(std::env::current_exe().unwrap().with_file_name("triggers.log"), text);
}
