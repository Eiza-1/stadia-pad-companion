//! The always-on bridge: finds the Stadia pad (USB or Bluetooth LE), hides it from games, and drives
//! one virtual Xbox 360 pad from its reports. Runs on its own thread; idle cost is one wake per second.

use crate::log;
use crate::mapping::{Mapper, XOut};
use crate::report::{parse_input, rumble_report, PadState};
use crate::settings::Settings;
use crate::special::{Action, Special};
use crate::stats::{Stats, StatsSnapshot};
use crate::win::hid::{enumerate_stadia, now_us, usb_stadia_present, HidDevice};
use crate::win::{hidhide, procs, vigem::VirtualPad};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transport {
    Usb,
    Bluetooth,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeviceInfo {
    pub path: String,
    pub instance_id: String,
    pub transport: Transport,
}

/// USB wins when the pad is both cabled and paired, so there is only ever one virtual pad.
pub fn choose_device(devs: &[DeviceInfo]) -> Option<&DeviceInfo> {
    devs.iter().find(|d| d.transport == Transport::Usb).or_else(|| devs.first())
}

/// `\\?\hid#vid_18d1&pid_9400&mi_01#7&abc&0&0000#{guid}` → `HID\VID_18D1&PID_9400&MI_01\7&ABC&0&0000`
pub fn to_instance_id(path: &str) -> String {
    let p = path.strip_prefix(r"\\?\").unwrap_or(path);
    let p = p.rfind("#{").map_or(p, |g| &p[..g]);
    p.replace('#', "\\").to_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    VigemMissing,
    HidHideMissing,
    Io(String),
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LowLatencyState {
    Unsupported,
    #[default]
    Off,
    Active,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Status {
    pub connection: Option<Transport>,
    pub battery: Option<u8>,
    pub charging: bool,
    pub vigem_ok: bool,
    pub hidhide_ok: bool,
    pub ble_low_latency: LowLatencyState,
    pub rate_override_available: bool,
    pub other_mappers: Vec<String>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            connection: None,
            battery: None,
            charging: false,
            vigem_ok: true,
            hidhide_ok: true,
            ble_low_latency: LowLatencyState::Off,
            rate_override_available: false,
            other_mappers: Vec::new(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum BridgeEvent {
    Pad { state: PadState, out: XOut },
    Stats(StatsSnapshot),
    Status(Status),
    Action(Action),
}

/// Live pad events are throttled to ~60/s; the UI cannot draw faster and the bridge stays light.
const PAD_EVENT_INTERVAL_US: u64 = 16_667;

struct Shared {
    settings: Mutex<Settings>,
    settings_dirty: AtomicBool,
    status: Mutex<Status>,
    sub: Mutex<Option<Sender<BridgeEvent>>>,
    stop: AtomicBool,
    rumble_pct: AtomicU8,
    hidden: Mutex<Vec<String>>,
    thread: Mutex<Option<thread::Thread>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn emit(&self, ev: BridgeEvent) {
        let mut sub = lock(&self.sub);
        if let Some(tx) = sub.as_ref() {
            if tx.send(ev).is_err() {
                *sub = None;
            }
        }
    }

    fn subscribed(&self) -> bool {
        lock(&self.sub).is_some()
    }

    fn set_status(&self, f: impl FnOnce(&mut Status)) {
        let s = {
            let mut st = lock(&self.status);
            f(&mut st);
            st.clone()
        };
        self.emit(BridgeEvent::Status(s));
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Sleeps up to `d`, returning early when stopped.
    fn idle(&self, d: Duration) {
        if !self.stopping() {
            thread::park_timeout(d);
        }
    }

    fn hide(&self, id: &str) {
        if hidhide::ensure_hidden(&[id.to_string()]) {
            let mut h = lock(&self.hidden);
            if !h.iter().any(|x| x == id) {
                h.push(id.to_string());
            }
        } else {
            log!("could not hide {id} from games");
        }
    }

    fn unhide_all(&self) {
        let ids = std::mem::take(&mut *lock(&self.hidden));
        hidhide::unhide(&ids);
    }
}

pub struct Bridge;

impl Bridge {
    pub fn start(settings: Settings) -> BridgeHandle {
        let shared = Arc::new(Shared {
            rumble_pct: AtomicU8::new(settings.active().rumble_pct.min(100)),
            settings: Mutex::new(settings),
            settings_dirty: AtomicBool::new(false),
            status: Mutex::new(Status::default()),
            sub: Mutex::new(None),
            stop: AtomicBool::new(false),
            hidden: Mutex::new(Vec::new()),
            thread: Mutex::new(None),
        });
        let sh = shared.clone();
        let t = thread::Builder::new().name("pad-bridge".into()).spawn(move || run(sh)).expect("spawn bridge thread");
        *lock(&shared.thread) = Some(t.thread().clone());
        BridgeHandle { shared, thread: Some(t) }
    }
}

pub struct BridgeHandle {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl BridgeHandle {
    pub fn update_settings(&self, s: Settings) {
        self.shared.rumble_pct.store(s.active().rumble_pct.min(100), Ordering::Relaxed);
        *lock(&self.shared.settings) = s;
        self.shared.settings_dirty.store(true, Ordering::Release);
    }

    pub fn settings(&self) -> Settings {
        lock(&self.shared.settings).clone()
    }

    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    pub fn subscribe(&self, tx: Sender<BridgeEvent>) {
        let _ = tx.send(BridgeEvent::Status(self.status()));
        *lock(&self.shared.sub) = Some(tx);
    }

    pub fn unsubscribe(&self) {
        *lock(&self.shared.sub) = None;
    }

    /// Stops the bridge, removes the virtual pad and gives the real pad back to every app.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = lock(&self.shared.thread).as_ref() {
            t.unpark();
        }
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for BridgeHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(sh: Arc<Shared>) {
    log!("bridge started");
    if hidhide::installed() && !hidhide::ensure_app_registered() {
        log!("HidHide did not whitelist us; a hidden pad will not be found");
    }
    let mut mapper_check = 0u64;
    while !sh.stopping() {
        let now = now_us();
        if now >= mapper_check {
            mapper_check = now + 30_000_000;
            let others = procs::other_mappers();
            let hh = hidhide::installed();
            if lock(&sh.status).other_mappers != others || lock(&sh.status).hidhide_ok != hh {
                sh.set_status(|s| {
                    s.other_mappers = others;
                    s.hidhide_ok = hh;
                });
            }
        }
        let devs = enumerate_stadia();
        match choose_device(&devs).cloned() {
            Some(dev) => {
                if let Err(e) = session(&sh, &dev) {
                    log!("session ended: {e:?}");
                    sh.set_status(|s| s.vigem_ok = e != BridgeError::VigemMissing);
                    sh.idle(Duration::from_secs(if e == BridgeError::VigemMissing { 5 } else { 1 }));
                }
            }
            None => sh.idle(Duration::from_secs(1)),
        }
    }
    sh.unhide_all();
    log!("bridge stopped");
}

fn session(sh: &Arc<Shared>, dev: &DeviceInfo) -> Result<(), BridgeError> {
    let io = |e: std::io::Error| BridgeError::Io(e.to_string());
    let settings = lock(&sh.settings).clone();
    if settings.hide_from_games {
        sh.hide(&dev.instance_id);
    }

    let input = HidDevice::open_shared(&dev.path).map_err(io)?;
    let writer = Arc::new(HidDevice::open_writer(&dev.path).map_err(io)?);
    let out_len = writer.output_len;

    let w = writer.clone();
    let sh2 = sh.clone();
    let mut last_rumble = None;
    let mut failure_logged = false;
    let mut pad = VirtualPad::connect(move |large, small| {
        let pct = sh2.rumble_pct.load(Ordering::Relaxed) as u32;
        let v = ((large as u32 * pct / 100) as u8, (small as u32 * pct / 100) as u8);
        if last_rumble == Some(v) || out_len == 0 {
            return;
        }
        last_rumble = Some(v);
        if !w.write_output(&rumble_report(v.0, v.1, out_len)) && !failure_logged {
            failure_logged = true;
            log!("rumble write failed: {}; further failures not logged", std::io::Error::last_os_error());
        }
    })?;

    let mut mapper = Mapper::new(settings.active().clone(), settings.pad_mode, settings.stadia);
    let mut special = Special::new(settings.assistant, settings.capture, settings.stadia);
    let mut stats = Stats::new(1024);
    let mut reader = input.reader(4).map_err(io)?;
    let mut hide = settings.hide_from_games;

    log!("connected {:?} {}", dev.transport, dev.instance_id);
    sh.set_status(|s| {
        s.vigem_ok = true;
        s.connection = Some(dev.transport);
        s.charging = dev.transport == Transport::Usb;
    });

    let mut last_state = PadState::default();
    let mut last_pad_event = 0u64;
    let mut next_tick = now_us() + 1_000_000;
    let result = loop {
        if sh.stopping() {
            break Ok(());
        }
        if sh.settings_dirty.swap(false, Ordering::Acquire) {
            let s = lock(&sh.settings).clone();
            mapper.set(s.active().clone(), s.pad_mode, s.stadia);
            special.set(s.assistant, s.capture, s.stadia);
            if s.hide_from_games != hide {
                hide = s.hide_from_games;
                if hide {
                    sh.hide(&dev.instance_id);
                } else {
                    sh.unhide_all();
                }
            }
        }

        let now_ms = now_us() / 1000;
        let timeout = special.next_deadline().map_or(1000, |d| d.saturating_sub(now_ms).min(1000)) as u32;
        let actions = match reader.next(timeout) {
            Ok(Some((data, arrival))) => match parse_input(&data) {
                Some(state) => {
                    let actions = special.update(&state, arrival / 1000);
                    let out = mapper.map(&state);
                    pad.submit(&out);
                    stats.record(arrival, now_us());
                    last_state = state;
                    if arrival.saturating_sub(last_pad_event) >= PAD_EVENT_INTERVAL_US && sh.subscribed() {
                        last_pad_event = arrival;
                        sh.emit(BridgeEvent::Pad { state, out });
                    }
                    actions
                }
                None => Vec::new(),
            },
            Ok(None) => special.update(&last_state, now_us() / 1000),
            Err(e) => break Err(io(e)),
        };
        for a in actions {
            log!("action {a:?}");
            sh.emit(BridgeEvent::Action(a));
        }

        let now = now_us();
        if now >= next_tick {
            next_tick = now + 1_000_000;
            if sh.subscribed() {
                sh.emit(BridgeEvent::Stats(stats.snapshot()));
            }
            // A cable plugged in while on Bluetooth: switch to USB (the outer loop picks it).
            if dev.transport == Transport::Bluetooth
                && usb_stadia_present()
                && enumerate_stadia().iter().any(|d| d.transport == Transport::Usb)
            {
                log!("USB pad appeared, switching from Bluetooth");
                break Ok(());
            }
        }
    };

    if out_len > 0 {
        writer.write_output(&rumble_report(0, 0, out_len));
    }
    drop(reader);
    drop(pad);
    log!("disconnected {:?}", dev.transport);
    sh.set_status(|s| {
        s.connection = None;
        s.battery = None;
        s.charging = false;
    });
    result.or_else(|e| if sh.stopping() { Ok(()) } else { Err(e) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(path: &str, transport: Transport) -> DeviceInfo {
        DeviceInfo { path: path.into(), instance_id: path.into(), transport }
    }

    #[test]
    fn prefers_usb() {
        let d = [dev("bt", Transport::Bluetooth), dev("usb", Transport::Usb)];
        assert_eq!(choose_device(&d).unwrap().path, "usb");
    }

    #[test]
    fn none_when_empty() {
        assert!(choose_device(&[]).is_none());
    }

    #[test]
    fn instance_id() {
        assert_eq!(
            to_instance_id(r"\\?\hid#vid_18d1&pid_9400&mi_01#7&abc&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"),
            r"HID\VID_18D1&PID_9400&MI_01\7&ABC&0&0000"
        );
    }
}
