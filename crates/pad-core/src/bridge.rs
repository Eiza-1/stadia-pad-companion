//! The always-on bridge: finds the Stadia pad (USB or Bluetooth LE), hides it from games, and drives
//! one virtual Xbox 360 pad from its reports. Runs on its own thread; idle cost is one wake per second.

use crate::log;
use crate::mapping::{Mapper, XOut};
use crate::report::{parse_input, rumble_report, PadState};
use crate::rumble::{Motors, RumbleMode, Translator, DEMO};
use crate::settings::Settings;
use crate::special::{Action, Special};
use crate::stats::{Stats, StatsSnapshot};
use crate::win::hid::{enumerate_stadia, now_us, stadia_instance_ids, usb_stadia_present, HidDevice, RawDeviceInfo};
use crate::settings::PadMode;
use crate::win::ble;
use crate::win::perf::CompetitiveGuard;
use crate::win::{hidhide, procs, vigem::VirtualPad};
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

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Status {
    pub connection: Option<Transport>,
    pub battery: Option<u8>,
    pub charging: bool,
    pub vigem_ok: bool,
    pub hidhide_ok: bool,
    /// Bluetooth connection interval Windows negotiated (ms); `None` on USB or when unreadable.
    pub link_ms: Option<f32>,
    /// Whether the connected pad accepts rumble (`false` over Bluetooth); `None` while disconnected.
    pub rumble: Option<bool>,
    pub other_mappers: Vec<String>,
    /// Identity of the connected pad (for the Home firmware panel); `None` while disconnected.
    pub device: Option<PadIdentity>,
}

/// What the Home firmware panel shows about the connected pad.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct PadIdentity {
    pub model: String,
    /// `VID:PID` in hex, e.g. `18D1:9400`.
    pub hardware_id: String,
    /// Device release number (bcdDevice), e.g. `1.00`.
    pub version: String,
    /// Firmware revision string from the BLE Device Information service; only readable over Bluetooth.
    pub firmware: Option<String>,
    /// `Some(true)` once seen over Bluetooth (only the Bluetooth-mode firmware can pair). Over USB both
    /// firmwares report the same ids, so it stays unknown.
    pub bluetooth_firmware: Option<bool>,
    pub serial: String,
}

pub fn identity(raw: &RawDeviceInfo, transport: Transport) -> PadIdentity {
    let v = raw.version;
    PadIdentity {
        model: if raw.product.trim().is_empty() { "Stadia Controller".into() } else { raw.product.trim().into() },
        hardware_id: format!("{:04X}:{:04X}", raw.vid, raw.pid),
        version: format!("{:x}.{:02x}", v >> 8, v & 0xFF),
        firmware: None,
        bluetooth_firmware: (transport == Transport::Bluetooth).then_some(true),
        serial: raw.serial.trim().into(),
    }
}

impl Default for Status {
    fn default() -> Self {
        Status {
            connection: None,
            battery: None,
            charging: false,
            vigem_ok: true,
            hidhide_ok: true,
            link_ms: None,
            rumble: None,
            other_mappers: Vec::new(),
            device: None,
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

/// Limits live pad events to the UI's frame rate without ever losing the newest state: a report that
/// comes too soon is held and sent once the interval has passed, even if the pad then goes quiet.
#[derive(Default)]
struct PadThrottle<T> {
    last: Option<u64>,
    held: Option<T>,
}

impl<T> PadThrottle<T> {
    /// A new report at `now` (µs): returns it if it may go out now, else holds it.
    fn report(&mut self, v: T, now: u64) -> Option<T> {
        if self.last.is_some_and(|l| now.saturating_sub(l) < PAD_EVENT_INTERVAL_US) {
            self.held = Some(v);
            return None;
        }
        self.last = Some(now);
        self.held = None;
        Some(v)
    }

    /// Called when no report came: releases the held state once its slot has arrived.
    fn tick(&mut self, now: u64) -> Option<T> {
        let due = self.last.is_none_or(|l| now.saturating_sub(l) >= PAD_EVENT_INTERVAL_US);
        if due && self.held.is_some() {
            self.last = Some(now);
            return self.held.take();
        }
        None
    }

    /// How long (ms, rounded up) until a held state is due; `None` when nothing is held.
    fn wake_in_ms(&self, now: u64) -> Option<u64> {
        self.held.as_ref()?;
        let l = self.last.unwrap_or(now);
        Some((l + PAD_EVENT_INTERVAL_US).saturating_sub(now).div_ceil(1000))
    }
}

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
    /// The session's rumble engine (USB only; `None` over Bluetooth or while disconnected).
    rumble: Mutex<Option<Sender<RumbleMsg>>>,
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
        crate::win::actions::set_capture_tool(settings.capture_tool);
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
            rumble: Mutex::new(None),
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
        if let Some(tx) = lock(&self.shared.rumble).as_ref() {
            let _ = tx.send(RumbleMsg::Config(s.active().rumble_mode, s.active().rumble_pct));
        }
        crate::win::actions::set_capture_tool(s.capture_tool);
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

    /// Plays the Test rumble scene (rumble::DEMO) through the selected rumble profile and strength.
    pub fn test_rumble(&self) {
        if let Some(tx) = lock(&self.shared.rumble).as_ref() {
            let _ = tx.send(RumbleMsg::Demo);
        }
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

/// Competitive mode while a pad is connected. Deliberately no Bluetooth connection-parameter request:
/// measured on the pad with nothing else holding one (bridge_cli --bench), Windows' own link is 10 ms;
/// ThroughputOptimized stretches it to 15 ms (more >12 ms gaps, worse max), Balanced to 60 ms and
/// PowerOptimized to 180 ms. The throughput preset also takes radio time from 2.4 GHz devices (mice).
#[derive(Default)]
struct Perf {
    guard: Option<CompetitiveGuard>,
}

impl Perf {
    fn apply(&mut self, s: &Settings) {
        match (s.pad_mode == PadMode::Competitive, self.guard.is_some()) {
            (true, false) => self.guard = Some(CompetitiveGuard::enter()),
            (false, true) => self.guard = None,
            _ => {}
        }
    }
}

/// Device-list checks that run once a second during a session. They walk Windows' device interfaces
/// (milliseconds, sometimes more), so they live on their own normal-priority thread: the input thread
/// only reads, maps and forwards.
struct Watcher {
    alive: Arc<AtomicBool>,
    hide: Arc<AtomicBool>,
    usb_appeared: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    fn spawn(sh: &Arc<Shared>, transport: Transport, hide: bool) -> Watcher {
        let alive = Arc::new(AtomicBool::new(true));
        let hide = Arc::new(AtomicBool::new(hide));
        let usb_appeared = Arc::new(AtomicBool::new(false));
        let (sh, a, h, u) = (sh.clone(), alive.clone(), hide.clone(), usb_appeared.clone());
        let thread = thread::Builder::new()
            .name("pad-watch".into())
            .spawn(move || {
                while a.load(Ordering::Relaxed) {
                    thread::park_timeout(Duration::from_secs(1));
                    if !a.load(Ordering::Relaxed) {
                        break;
                    }
                    // A second transport came up (e.g. Bluetooth while cabled): hide it too.
                    if h.load(Ordering::Relaxed) {
                        sh.hide(&stadia_instance_ids());
                    }
                    // A cable plugged in while on Bluetooth: the session ends and the outer loop picks USB.
                    if transport == Transport::Bluetooth
                        && usb_stadia_present()
                        && enumerate_stadia().iter().any(|d| d.transport == Transport::Usb)
                    {
                        u.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            })
            .ok();
        Watcher { alive, hide, usb_appeared, thread }
    }

    fn stop(&self) {
        self.alive.store(false, Ordering::Relaxed);
        if let Some(t) = &self.thread {
            t.thread().unpark();
        }
    }
}

impl Drop for Watcher {
    /// Not joined: a hide may be waiting on HidHide (or its admin prompt); the thread exits on its own.
    fn drop(&mut self) {
        self.stop();
    }
}

/// Bluetooth status off the input thread: the firmware string once, the link interval every 30 s (asks
/// Windows, no radio traffic) and the battery every 5 min (an over-the-air GATT read shares the link with
/// the pad's reports, so it stays rare).
fn spawn_battery(sh: &Arc<Shared>, addr: u64, alive: Arc<AtomicBool>) -> Option<JoinHandle<()>> {
    let sh = sh.clone();
    thread::Builder::new()
        .name("pad-battery".into())
        .spawn(move || {
            com_init();
            // Once per connection: the firmware string never changes while connected.
            if let Some(fw) = ble::firmware_revision(addr) {
                if alive.load(Ordering::Relaxed) {
                    sh.set_status(|s| if let Some(d) = s.device.as_mut() { d.firmware = Some(fw) });
                }
            }
            let mut tick = 0u32;
            while alive.load(Ordering::Relaxed) {
                let link = ble::link_params(addr).map(|l| l.interval_ms);
                let level = if tick % 10 == 0 { Some(ble::battery(addr)) } else { None };
                if alive.load(Ordering::Relaxed) {
                    let st = lock(&sh.status).clone();
                    let level = level.unwrap_or(st.battery);
                    if st.battery != level || st.link_ms != link {
                        sh.set_status(|s| {
                            s.battery = level;
                            s.link_ms = link;
                        });
                    }
                }
                tick = tick.wrapping_add(1);
                thread::park_timeout(Duration::from_secs(30));
            }
        })
        .ok()
}

/// Messages to a session's rumble engine.
enum RumbleMsg {
    /// The game set new motor levels.
    Game(Motors),
    /// Rumble profile / strength changed.
    Config(RumbleMode, u8),
    /// Play the Test rumble scene through the current profile.
    Demo,
    Stop,
}

/// The session's Rumble Translation Engine: translates each game request, times Competitive ticks and
/// the Test rumble scene, and writes the pad only when the motor output changes.
fn spawn_rumble(writer: Arc<HidDevice>, out_len: usize, mode: RumbleMode, strength: u8) -> (Sender<RumbleMsg>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<RumbleMsg>();
    let thread = thread::Builder::new()
        .name("pad-rumble".into())
        .spawn(move || {
            let mut t = Translator::new(mode, strength);
            let mut written = Motors::OFF;
            let mut failure_logged = false;
            let mut write = |m: Motors| {
                if m == written {
                    return;
                }
                written = m;
                if !writer.write_output(&rumble_report(m.heavy, m.light, out_len)) && !failure_logged {
                    failure_logged = true;
                    log!("rumble write failed: {}; further failures not logged", std::io::Error::last_os_error());
                }
            };
            let mut game = Motors::OFF;
            // Test rumble scene in progress: (start µs, next step).
            let mut demo: Option<(u64, usize)> = None;
            loop {
                let now = now_us();
                let demo_due = demo.and_then(|(st, i)| DEMO.get(i).map(|(ms, _)| st + *ms as u64 * 1000));
                let wake = [t.next_deadline(), demo_due].into_iter().flatten().min();
                let timeout = wake.map_or(Duration::from_secs(3600), |w| Duration::from_micros(w.saturating_sub(now)));
                match rx.recv_timeout(timeout) {
                    Ok(RumbleMsg::Game(m)) => {
                        game = m;
                        if demo.is_none() {
                            write(t.request(m, now_us()));
                        }
                    }
                    Ok(RumbleMsg::Config(mode, strength)) => {
                        t.set(mode, strength);
                        if demo.is_none() {
                            write(t.request(game, now_us()));
                        }
                    }
                    Ok(RumbleMsg::Demo) => {
                        t.reset();
                        demo = Some((now_us(), 0));
                    }
                    Ok(RumbleMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let now = now_us();
                if let Some(m) = t.tick(now) {
                    write(m);
                }
                if let Some((st, i)) = demo.as_mut() {
                    while let Some((ms, m)) = DEMO.get(*i) {
                        if now < *st + *ms as u64 * 1000 {
                            break;
                        }
                        write(t.request(*m, now));
                        *i += 1;
                    }
                    if *i >= DEMO.len() && t.next_deadline().is_none() {
                        // Scene over: back to whatever the game is asking for.
                        demo = None;
                        t.reset();
                        write(t.request(game, now));
                    }
                }
            }
            write(Motors::OFF);
        })
        .expect("spawn rumble engine");
    (tx, thread)
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
            let changed = {
                let st = lock(&sh.status);
                st.other_mappers != others || st.hidhide_ok != hh
            };
            if changed {
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
    // Rumble is probed with an all-motors-off report (harmless). Over Bluetooth Windows rejects the pad's
    // rumble report (error 87; apps are denied the HID GATT service, so there is no way around the
    // driver; SDL disables Stadia rumble on Bluetooth too), so the session skips rumble entirely.
    let rumble_ok = out_len > 0 && writer.write_output(&rumble_report(0, 0, out_len));
    if !rumble_ok {
        log!("rumble unavailable on {:?}: {}", dev.transport, std::io::Error::last_os_error());
    }
    // Game rumble goes through the Rumble Translation Engine on its own thread: the ViGEm callback only
    // queues the request, so a game's rumble call never waits on USB or on a pulse timer.
    let engine = rumble_ok
        .then(|| spawn_rumble(writer.clone(), out_len, settings.active().rumble_mode, settings.active().rumble_pct));
    *lock(&sh.rumble) = engine.as_ref().map(|(tx, _)| tx.clone());
    let game_tx = engine.as_ref().map(|(tx, _)| tx.clone());
    let mut pad = VirtualPad::connect(move |large, small| {
        if let Some(tx) = &game_tx {
            let _ = tx.send(RumbleMsg::Game(Motors::new(large, small)));
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
    perf.apply(&settings);

    log!("connected {:?} {}", dev.transport, dev.instance_id);
    let ident = input.device_info().map(|r| identity(&r, dev.transport));
    sh.set_status(|s| {
        s.vigem_ok = true;
        s.connection = Some(dev.transport);
        s.rumble = Some(rumble_ok);
        s.device = ident;
        s.charging = dev.transport == Transport::Usb;
    });

    // After the status above, so the firmware read always finds `device` set.
    let battery_alive = Arc::new(AtomicBool::new(true));
    let battery_thread = ble::bt_addr_from_path(&dev.path)
        .filter(|_| dev.transport == Transport::Bluetooth)
        .and_then(|a| spawn_battery(sh, a, battery_alive.clone()));

    let mut last_state = PadState::default();
    let mut throttle: PadThrottle<(PadState, XOut)> = PadThrottle::default();
    let mut next_tick = now_us() + 1_000_000;
    let watch = Watcher::spawn(sh, dev.transport, hide);
    let result = loop {
        if sh.stopping() {
            break Ok(());
        }
        if sh.settings_dirty.swap(false, Ordering::Acquire) {
            let s = lock(&sh.settings).clone();
            mapper.set(s.active().clone(), s.pad_mode, s.stadia);
            special.set(s.assistant, s.capture, s.stadia);
            perf.apply(&s);
            if s.hide_from_games != hide {
                hide = s.hide_from_games;
                watch.hide.store(hide, Ordering::Relaxed);
                if hide {
                    sh.hide(&ids_to_hide(&enumerate_stadia()));
                } else {
                    sh.unhide_all();
                }
            }
        }

        let now_ms = now_us() / 1000;
        let mut timeout = special.next_deadline().map_or(1000, |d| d.saturating_sub(now_ms).min(1000)) as u32;
        if let Some(due) = throttle.wake_in_ms(now_us()) {
            // Wake in time to send the held-back pad event if no newer report comes first.
            timeout = timeout.min(due as u32);
        }
        let actions = match reader.next(timeout) {
            Ok(Some((data, arrival))) => match parse_input(&data) {
                Some(state) => {
                    let actions = special.update(&state, arrival / 1000);
                    let out = mapper.map(&state);
                    pad.submit(&out);
                    stats.record(arrival, now_us());
                    last_state = state;
                    if let Some((state, out)) = throttle.report((state, out), arrival) {
                        if sh.subscribed() {
                            sh.emit(BridgeEvent::Pad { state, out });
                        }
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
        if let Some((state, out)) = throttle.tick(now) {
            if sh.subscribed() {
                sh.emit(BridgeEvent::Pad { state, out });
            }
        }
        if now >= next_tick {
            next_tick = now + 1_000_000;
            if sh.subscribed() {
                sh.emit(BridgeEvent::Stats(stats.snapshot()));
            }
        }
        if watch.usb_appeared.load(Ordering::Relaxed) {
            log!("USB pad appeared, switching from Bluetooth");
            break Ok(());
        }
    };
    watch.stop();

    *lock(&sh.rumble) = None;
    drop(reader);
    // Drop the virtual pad first (joins ViGEm's notification thread), so no game rumble arrives after
    // the engine stops; the engine then turns the motors off and exits.
    drop(pad);
    if let Some((tx, thread)) = engine {
        let _ = tx.send(RumbleMsg::Stop);
        let _ = thread.join();
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
        s.device = None;
        s.charging = false;
        s.link_ms = None;
        s.rumble = None;
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


    fn raw(version: u16, product: &str) -> RawDeviceInfo {
        RawDeviceInfo { vid: 0x18D1, pid: 0x9400, version, product: product.into(), serial: "99180YCAC5Q7QD".into() }
    }

    #[test]
    fn identity_formats_ids_and_bcd_version() {
        let id = identity(&raw(0x0100, "Stadia Controller rev. A"), Transport::Usb);
        assert_eq!(id.model, "Stadia Controller rev. A");
        assert_eq!(id.hardware_id, "18D1:9400");
        assert_eq!(id.version, "1.00");
        assert_eq!(id.serial, "99180YCAC5Q7QD");
        assert_eq!(identity(&raw(0x0213, "x"), Transport::Usb).version, "2.13");
    }

    #[test]
    fn bluetooth_firmware_is_only_claimed_when_seen_over_bluetooth() {
        assert_eq!(identity(&raw(0x0100, "x"), Transport::Bluetooth).bluetooth_firmware, Some(true));
        assert_eq!(identity(&raw(0x0100, "x"), Transport::Usb).bluetooth_firmware, None, "USB can't tell the two firmwares apart");
    }

    #[test]
    fn identity_falls_back_when_the_pad_sends_no_product_string() {
        assert_eq!(identity(&raw(0x0100, ""), Transport::Usb).model, "Stadia Controller");
    }

    #[test]
    fn throttle_never_loses_the_last_state() {
        let mut t = PadThrottle::default();
        assert_eq!(t.report("press", 0), Some("press"), "first report goes straight out");
        assert_eq!(t.report("release", 5_000), None, "too soon: held");
        assert_eq!(t.tick(10_000), None, "still inside the interval");
        assert_eq!(t.wake_in_ms(10_000), Some(7), "wakes when the slot opens");
        assert_eq!(t.tick(16_667), Some("release"), "held state goes out once due, with no new report");
        assert_eq!(t.tick(40_000), None, "sent once");
        assert_eq!(t.wake_in_ms(40_000), None);
    }

    #[test]
    fn fast_full_trigger_press_reaches_the_ui_while_held() {
        // Measured on the pad: a full L2/R2 press ramps 0 -> 255 in a few 8 ms reports, then the pad goes
        // quiet while the trigger is held. The UI must end on 255, not on a mid-ramp value.
        let mut t = PadThrottle::default();
        let mut shown = None;
        for (i, raw) in [0u8, 90, 180, 240, 255].into_iter().enumerate() {
            if let Some(v) = t.report(raw, i as u64 * 8_000) {
                shown = Some(v);
            }
        }
        assert_ne!(shown, Some(255), "the 60/s limit holds the last report back at first");
        let mut now = 32_000;
        while let Some(wait) = t.wake_in_ms(now) {
            now += wait * 1000;
            if let Some(v) = t.tick(now) {
                shown = Some(v);
            }
        }
        assert_eq!(shown, Some(255), "the held full press is shown with no further reports");
        assert!(now <= 32_000 + 2 * PAD_EVENT_INTERVAL_US, "within two UI frames");
    }

    #[test]
    fn throttle_newer_report_replaces_the_held_one() {
        let mut t = PadThrottle::default();
        t.report(1, 0);
        assert_eq!(t.report(2, 4_000), None);
        assert_eq!(t.report(3, 8_000), None);
        assert_eq!(t.report(4, 20_000), Some(4), "a due report goes out and clears the held one");
        assert_eq!(t.tick(60_000), None);
    }
}
