//! The always-on bridge: finds the Stadia pad (USB or Bluetooth LE), hides it from games, and drives
//! one virtual Xbox 360 pad from its reports. Runs on its own thread; idle cost is one wake per second.

use crate::log;
use crate::mapping::{Mapper, XOut};
use crate::report::{parse_input, rumble_report, PadState};
use crate::settings::Settings;
use crate::special::{Action, Special};
use crate::stats::{Stats, StatsSnapshot};
use crate::win::hid::{enumerate_stadia, now_us, stadia_instance_ids, usb_stadia_present, HidDevice};
use crate::settings::PadMode;
use crate::win::ble::{self, LowLatency};
use crate::win::perf::CompetitiveGuard;
use crate::win::{hidhide, procs, rate, vigem::VirtualPad};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
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
/// Every Stadia device present is hidden, not just the one in use: a pad both cabled and paired
/// exposes two HID devices, and Steam would show the unused one next to the Xbox pad.
pub fn ids_to_hide(devs: &[DeviceInfo]) -> Vec<String> {
    devs.iter().map(|d| d.instance_id.clone()).collect()
}

/// Remembers a refused BLE low-latency request so settings saves don't re-run the blocking request on
/// the input thread; a different pad (reconnect) or toggling the setting retries.
#[derive(Default)]
struct LlRetry {
    refused_for: Option<Option<u64>>,
    asked: Option<u64>,
}

impl LlRetry {
    fn should_request(&mut self, addr: Option<u64>) -> bool {
        self.asked = addr;
        self.refused_for != Some(addr)
    }
    fn failed(&mut self) {
        self.refused_for = Some(self.asked);
    }
    fn reset(&mut self) {
        self.refused_for = None;
    }
}

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
    /// Ids HidHide refused to hide this run; not retried every second.
    hide_failed: Mutex<Vec<String>>,
    /// Actions run on their own thread so launching Edge or SendInput never stalls input.
    actions: Mutex<Sender<Action>>,
    thread: Mutex<Option<thread::Thread>>,
    /// Live pad/stats events are produced only while a window is showing them.
    live: AtomicBool,
    /// Rumble handle of the current session, for the settings page's rumble test.
    writer: Mutex<Option<(Arc<HidDevice>, usize)>>,
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
        self.live.load(Ordering::Relaxed) && lock(&self.sub).is_some()
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

    /// Hides the given pads from games (only ids not already hidden by us this run).
    fn hide(&self, ids: &[String]) {
        let new: Vec<String> = {
            let (h, f) = (lock(&self.hidden), lock(&self.hide_failed));
            ids.iter().filter(|id| !h.contains(id) && !f.contains(id)).cloned().collect()
        };
        if new.is_empty() {
            return;
        }
        if hidhide::ensure_hidden(&new) {
            lock(&self.hidden).extend(new);
        } else {
            log!("could not hide {new:?} from games");
            lock(&self.hide_failed).extend(new);
        }
    }

    /// Gives every Stadia pad back to other apps, including entries left by a crash or another tool.
    fn unhide_all(&self) {
        let ids = std::mem::take(&mut *lock(&self.hidden));
        lock(&self.hide_failed).clear();
        hidhide::unhide(&ids);
        hidhide::unhide_all_stadia();
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
            hide_failed: Mutex::new(Vec::new()),
            actions: Mutex::new(spawn_action_worker()),
            live: AtomicBool::new(true),
            writer: Mutex::new(None),
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

    /// Turns the ~60/s pad and 1/s stats events on (window open) or off (window closed).
    /// Status and action events always flow to the subscriber.
    pub fn set_live(&self, on: bool) {
        self.shared.live.store(on, Ordering::Relaxed);
    }

    /// Short rumble pulse at the profile's strength, so the user can feel the setting.
    pub fn test_rumble(&self) {
        let Some((w, len)) = lock(&self.shared.writer).clone() else { return };
        let pct = self.shared.rumble_pct.load(Ordering::Relaxed) as u32;
        let v = (200 * pct / 100) as u8;
        thread::spawn(move || {
            w.write_output(&rumble_report(v, v, len));
            thread::sleep(Duration::from_millis(350));
            w.write_output(&rumble_report(0, 0, len));
        });
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

/// Competitive-mode resources for one session; rebuilt whenever the mode settings change.
#[derive(Default)]
struct Perf {
    guard: Option<CompetitiveGuard>,
    low_latency: Option<LowLatency>,
    retry: LlRetry,
}

impl Perf {
    fn apply(&mut self, s: &Settings, dev: &DeviceInfo) -> LowLatencyState {
        let competitive = s.pad_mode == PadMode::Competitive;
        match (competitive, self.guard.is_some()) {
            (true, false) => self.guard = Some(CompetitiveGuard::enter()),
            (false, true) => self.guard = None,
            _ => {}
        }
        let want_ll = competitive && s.ble_low_latency;
        let addr = ble::bt_addr_from_path(&dev.path).filter(|_| dev.transport == Transport::Bluetooth);
        if !ble::low_latency_supported() {
            return LowLatencyState::Unsupported;
        }
        match (want_ll, addr) {
            (true, Some(a)) => {
                if self.low_latency.is_none() {
                    if !self.retry.should_request(Some(a)) {
                        return LowLatencyState::Off;
                    }
                    match LowLatency::request(a) {
                        Ok(ll) => self.low_latency = Some(ll),
                        Err(state) => {
                            self.retry.failed();
                            return state;
                        }
                    }
                }
                LowLatencyState::Active
            }
            _ => {
                self.low_latency = None;
                self.retry.reset();
                LowLatencyState::Off
            }
        }
    }
}

/// Reads the BLE battery level on connect and every 60 s, off the input thread (GATT reads can take 100s of ms).
fn spawn_battery(sh: &Arc<Shared>, addr: u64, alive: Arc<AtomicBool>) -> Option<JoinHandle<()>> {
    let sh = sh.clone();
    thread::Builder::new()
        .name("pad-battery".into())
        .spawn(move || {
            com_init();
            while alive.load(Ordering::Relaxed) {
                let level = ble::battery(addr);
                if alive.load(Ordering::Relaxed) && lock(&sh.status).battery != level {
                    sh.set_status(|s| s.battery = level);
                }
                thread::park_timeout(Duration::from_secs(60));
            }
        })
        .ok()
}

/// Runs special-button actions; ends when the bridge's sender is dropped.
fn spawn_action_worker() -> Sender<Action> {
    let (tx, rx) = mpsc::channel::<Action>();
    let _ = thread::Builder::new().name("pad-actions".into()).spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        }
        for a in rx {
            crate::win::actions::run(a);
        }
    });
    tx
}

fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn run(sh: Arc<Shared>) {
    log!("bridge started");
    com_init();
    if hidhide::installed() && !hidhide::ensure_app_registered() {
        log!("HidHide did not whitelist us; a hidden pad will not be found");
    }
    let mut mapper_check = 0u64;
    let mut released_for_missing_vigem = false;
    while !sh.stopping() {
        let now = now_us();
        if now >= mapper_check {
            mapper_check = now + 30_000_000;
            let others = procs::other_mappers();
            let hh = hidhide::installed();
            let rate_ok = rate::override_driver_present();
            let changed = {
                let st = lock(&sh.status);
                st.other_mappers != others || st.hidhide_ok != hh || st.rate_override_available != rate_ok
            };
            if changed {
                sh.set_status(|s| {
                    s.other_mappers = others;
                    s.hidhide_ok = hh;
                    s.rate_override_available = rate_ok;
                });
            }
        }
        let devs = enumerate_stadia();
        match choose_device(&devs).cloned() {
            Some(dev) => {
                if let Err(e) = session(&sh, &dev) {
                    log!("session ended: {e:?}");
                    // No virtual pad possible: give the real pad back (once) so the user still has a controller.
                    if e == BridgeError::VigemMissing && !released_for_missing_vigem {
                        released_for_missing_vigem = true;
                        sh.unhide_all();
                    }
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

    let input = HidDevice::open_shared(&dev.path).map_err(io)?;
    let writer = Arc::new(HidDevice::open_writer(&dev.path).map_err(io)?);
    let out_len = writer.output_len;
    *lock(&sh.writer) = Some((writer.clone(), out_len));

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
    // Hide only once the virtual pad exists: without ViGEm the user keeps the real pad (Steam can still use it).
    let mut hide = settings.hide_from_games;
    if hide {
        sh.hide(&ids_to_hide(&enumerate_stadia()));
    } else {
        hidhide::unhide_all_stadia();
    }

    let mut mapper = Mapper::new(settings.active().clone(), settings.pad_mode, settings.stadia);
    let mut special = Special::new(settings.assistant, settings.capture, settings.stadia);
    let mut stats = Stats::new(1024);
    let mut reader = input.reader(4).map_err(io)?;
    let mut perf = Perf::default();
    let ll_state = perf.apply(&settings, dev);

    let battery_alive = Arc::new(AtomicBool::new(true));
    let battery_thread = ble::bt_addr_from_path(&dev.path)
        .filter(|_| dev.transport == Transport::Bluetooth)
        .and_then(|a| spawn_battery(sh, a, battery_alive.clone()));

    log!("connected {:?} {}", dev.transport, dev.instance_id);
    sh.set_status(|s| {
        s.vigem_ok = true;
        s.connection = Some(dev.transport);
        s.charging = dev.transport == Transport::Usb;
        s.ble_low_latency = ll_state;
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
            let ll = perf.apply(&s, dev);
            if lock(&sh.status).ble_low_latency != ll {
                sh.set_status(|st| st.ble_low_latency = ll);
            }
            if s.hide_from_games != hide {
                hide = s.hide_from_games;
                if hide {
                    sh.hide(&ids_to_hide(&enumerate_stadia()));
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
            if a != Action::OpenCompanion {
                let _ = lock(&sh.actions).send(a);
            }
            sh.emit(BridgeEvent::Action(a));
        }

        let now = now_us();
        if now >= next_tick {
            next_tick = now + 1_000_000;
            if sh.subscribed() {
                sh.emit(BridgeEvent::Stats(stats.snapshot()));
            }
            // A second transport came up (e.g. Bluetooth while cabled): hide it too.
            if hide {
                sh.hide(&stadia_instance_ids());
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

    *lock(&sh.writer) = None;
    drop(reader);
    // Drop the virtual pad first (joins the rumble thread), so no late rumble lands after the stop.
    drop(pad);
    if out_len > 0 {
        writer.write_output(&rumble_report(0, 0, out_len));
    }
    drop(perf);
    // Not joined: a GATT read in flight may take seconds; the thread checks `alive` before touching status.
    battery_alive.store(false, Ordering::Relaxed);
    if let Some(t) = battery_thread {
        t.thread().unpark();
    }
    log!("disconnected {:?}", dev.transport);
    sh.set_status(|s| {
        s.connection = None;
        s.battery = None;
        s.charging = false;
        if s.ble_low_latency == LowLatencyState::Active {
            s.ble_low_latency = LowLatencyState::Off;
        }
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

    #[test]
    fn hides_every_transport_not_just_the_chosen_one() {
        let d = [dev("bt", Transport::Bluetooth), dev("usb", Transport::Usb)];
        assert_eq!(ids_to_hide(&d), vec!["bt".to_string(), "usb".to_string()]);
    }

    #[test]
    fn low_latency_not_retried_until_inputs_change() {
        let mut r = LlRetry::default();
        assert!(r.should_request(Some(1)));
        r.failed();
        assert!(!r.should_request(Some(1)), "a settings save must not re-block input on a refused request");
        assert!(r.should_request(Some(2)), "a new pad / reconnect retries");
        r.failed();
        r.reset();
        assert!(r.should_request(Some(2)), "toggling the setting retries");
    }
}
