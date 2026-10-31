# Pad Companion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust/Tauri Windows app that bridges a Stadia controller (USB or BLE) to one virtual Xbox 360 pad with low latency, plus a Material You companion UI.

**Architecture:** Cargo workspace. `crates/pad-core` holds pure, unit-tested logic (report parsing, mapping, special-button state machine, stats, settings) plus a `win` module with Win32/WinRT glue (HID, HidHide, ViGEm, BLE, perf, actions) and the `bridge` thread. `app/src-tauri` is the Tauri 2 shell (tray, window lifecycle, IPC); `app/ui` is plain HTML/CSS/JS ported from the approved mockup.

**Tech Stack:** Rust stable (MSVC), `windows` crate (Win32 + WinRT Bluetooth), `vigem-client`, `serde`/`serde_json`, `bitflags`, Tauri 2 + `tauri-plugin-single-instance`; UI without framework or bundler.

**Spec:** `docs/superpowers/specs/2026-10-31-pad-companion-design.md`

## Global Constraints

- Windows 10 1903+ runs; Force Bluetooth Low-Latency Mode needs Windows 11 and is disabled otherwise.
- Pad opened shared (`FILE_SHARE_READ|FILE_SHARE_WRITE`), never exclusive.
- Exactly one virtual Xbox 360 pad, only while a physical pad is connected; USB preferred over BLE.
- Rumble via `WriteFile` of report `0x05` (5 bytes: id, large LE16, small LE16); `HidD_SetOutputReport` only as fallback.
- Idle CPU ≈ 0: no loop wakes faster than 1 s while no pad is connected; no UI events emitted while no window exists.
- Settings: `%LOCALAPPDATA%\PadCompanion\settings.json`; log `%LOCALAPPDATA%\PadCompanion\log.txt`, capped at 1 MB (rotate to `log.old.txt`).
- Window is destroyed on close; tray Quit stops the bridge and un-hides the pad.
- Polling rates offered: 125, 250, 500, 1000 Hz; 500/1000 badged EXPERIMENTAL; anything above 125 disabled unless a rate-override driver is detected (`HKLM\SYSTEM\CurrentControlSet\Services\hidusbf` exists) and always disabled on Bluetooth.
- Hold thresholds: Capture hold ≥ 500 ms = toggle recording; Stadia hold ≥ 1000 ms = open companion.
- Fonts Roboto Flex and Material Symbols Rounded bundled locally; Stadia and Gemini logos from the user's SVGs.
- Commit messages have no AI attribution lines (user preference).

## Review Focus

1. Pad switched USB↔Bluetooth or unplugged mid-game → old virtual pad removed, new one created, never two. Pinned in Task 6 (`choose_device` unit test + hardware check).
2. Cable plugged in while BLE also connected → USB used, one virtual pad. Pinned in Task 6 (`choose_device` prefers USB).
3. `settings.json` corrupt, empty or from an older version → defaults load, app starts, file rewritten on next save. Pinned in Task 2.
4. ViGEmBus or HidHide not installed → app still starts, UI shows what's missing with an install link, no crash loop. Pinned in Task 9 (`status_from_vigem_missing` unit test) and Task 10 (missing-driver banner).
5. Window opened and closed many times → bridge unaffected, WebView2 processes exit each time, RAM returns to baseline. Pinned in Task 9 verification.

---

### Task 1: Workspace + report parsing

**Files:**
- Create: `Cargo.toml` (workspace: `crates/pad-core`, `app/src-tauri`), `.gitignore` (`target/`, `node_modules/`), `crates/pad-core/Cargo.toml`, `crates/pad-core/src/lib.rs`, `crates/pad-core/src/report.rs`

**Interfaces:**
- Produces:
  - `bitflags! pub struct Buttons: u32 { A, B, X, Y, LB, RB, L3, R3, L2_CLICK, R2_CLICK, OPTIONS, MENU, STADIA, ASSISTANT, CAPTURE, DPAD_UP, DPAD_DOWN, DPAD_LEFT, DPAD_RIGHT }` (serde as list of names)
  - `#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize)] pub struct PadState { pub buttons: Buttons, pub lx: u8, pub ly: u8, pub rx: u8, pub ry: u8, pub l2: u8, pub r2: u8 }` (raw values, Y down-positive)
  - `pub fn parse_input(r: &[u8]) -> Option<PadState>` — `None` if `len < 10` or `r[0] != 0x03`
  - `pub fn rumble_report(large: u8, small: u8, len: usize) -> Vec<u8>` — length `max(5, len)`, motor value = `x as u16 * 257` LE

- [ ] **Step 1: `git init` in `PadCompanion`, create workspace, write failing tests in `report.rs`**

```rust
#[test] fn rejects_short_or_wrong_id() { assert!(parse_input(&[3;9]).is_none()); assert!(parse_input(&[1,8,0,0,128,128,128,128,0,0]).is_none()); }
#[test] fn neutral() { let s = parse_input(&[3,8,0,0,128,128,128,128,0,0]).unwrap(); assert_eq!(s.buttons, Buttons::empty()); assert_eq!((s.lx,s.ly,s.l2), (128,128,0)); }
#[test] fn hat_diagonals() { let ur = parse_input(&[3,1,0,0,128,128,128,128,0,0]).unwrap(); assert_eq!(ur.buttons, Buttons::DPAD_UP|Buttons::DPAD_RIGHT); let l = parse_input(&[3,6,0,0,128,128,128,128,0,0]).unwrap(); assert_eq!(l.buttons, Buttons::DPAD_LEFT); }
#[test] fn bits() { let s = parse_input(&[3,8,0x13,0x41,0,255,10,20,30,40]).unwrap();
  assert_eq!(s.buttons, Buttons::STADIA|Buttons::ASSISTANT|Buttons::CAPTURE|Buttons::A|Buttons::L3);
  assert_eq!((s.lx,s.ly,s.rx,s.ry,s.l2,s.r2),(0,255,10,20,30,40)); }
#[test] fn rumble() { assert_eq!(rumble_report(255,128,5), vec![5,0xFF,0xFF,0x80,0x80]); assert_eq!(rumble_report(0,0,8).len(), 8); }
```

- [ ] **Step 2:** `cargo test -p pad-core` → FAIL (unresolved items)
- [ ] **Step 3:** implement per the report map in the spec
- [ ] **Step 4:** `cargo test -p pad-core` → PASS
- [ ] **Step 5:** commit `feat(core): parse Stadia input reports and build rumble reports`

### Task 2: Settings and profiles

**Files:** Create `crates/pad-core/src/settings.rs`

**Interfaces:**
- Produces (all `Serialize, Deserialize, Clone, PartialEq, Debug`, `#[serde(default)]` on every struct):
  - `enum PadMode { Basic, Competitive }` · `enum PadColor { JustBlack, ClearlyWhite, Wasabi, NightBlue }` · `enum Curve { Linear, Smooth, Aggressive }`
  - `enum AssistantAction { Gemini, Off }` · `enum CaptureAction { TapHold, Off }` · `enum StadiaAction { GuideHold, GuideOnly }`
  - `enum XButton { A, B, X, Y, LB, RB, L3, R3, Back, Start, Guide, DpadUp, DpadDown, DpadLeft, DpadRight }`
  - `enum Target { Xbox(XButton), Disabled }`
  - `struct StickCfg { deadzone_pct: u8 /*default 8*/, curve: Curve /*Linear*/ }`
  - `struct TriggerCfg { start_pct: u8 /*4*/, end_pct: u8 /*96*/, hair: bool }`
  - `struct Profile { name: String, remap: BTreeMap<String, Target> /*key = Buttons flag name*/, left: StickCfg, right: StickCfg, triggers: TriggerCfg, rumble_pct: u8 /*100*/ }`
  - `struct Settings { active_profile: String /*"Default"*/, profiles: Vec<Profile> /*[Default]*/, pad_mode: PadMode, ble_low_latency: bool /*true*/, polling_hz: u16 /*125*/, pad_color: PadColor, hide_from_games: bool /*true*/, start_with_windows: bool /*false*/, assistant: AssistantAction, capture: CaptureAction, stadia: StadiaAction }`
  - `impl Settings { pub fn active(&self) -> &Profile; pub fn load(path: &Path) -> Settings; pub fn save(&self, path: &Path) -> io::Result<()>; pub fn default_path() -> PathBuf }`
  - `load` never fails: missing/corrupt → `Settings::default()`; ensures a "Default" profile exists and `active_profile` names an existing profile (else "Default"). `save` writes to `settings.json.tmp` then renames.

- [ ] **Step 1: failing tests**

```rust
#[test] fn roundtrip() { let d = tempdir(); let p = d.join("s.json"); let mut s = Settings::default(); s.pad_mode = PadMode::Competitive; s.save(&p).unwrap(); assert_eq!(Settings::load(&p), s); }
#[test] fn corrupt_or_empty_gives_defaults() { for c in ["", "{not json", "[]"] { let p = write_tmp(c); assert_eq!(Settings::load(&p), Settings::default()); } }
#[test] fn partial_file_fills_defaults() { let p = write_tmp(r#"{"pad_mode":"Competitive"}"#); let s = Settings::load(&p); assert_eq!(s.pad_mode, PadMode::Competitive); assert_eq!(s.polling_hz, 125); assert_eq!(s.active().name, "Default"); }
#[test] fn dangling_active_profile_falls_back() { let p = write_tmp(r#"{"active_profile":"Gone","profiles":[]}"#); assert_eq!(Settings::load(&p).active().name, "Default"); }
```
(`tempdir`/`write_tmp`: small test helpers using `std::env::temp_dir()` + unique name; no extra crate.)

- [ ] **Step 2:** run → FAIL · **Step 3:** implement · **Step 4:** run → PASS
- [ ] **Step 5:** commit `feat(core): settings with profiles and safe loading`

### Task 3: Mapping (remap, sticks, triggers, Basic smoothing)

**Files:** Create `crates/pad-core/src/mapping.rs`

**Interfaces:**
- Consumes: `PadState`, `Buttons` (Task 1); `Profile`, `PadMode`, `Target`, `XButton`, `StadiaAction` (Task 2)
- Produces:
  - `#[derive(Default, Clone, Copy, PartialEq, Debug, Serialize)] pub struct XOut { pub buttons: u16 /*XUSB bit layout*/, pub lt: u8, pub rt: u8, pub lx: i16, pub ly: i16, pub rx: i16, pub ry: i16 }`
  - `pub fn xusb_bit(b: XButton) -> u16`: DpadUp 0x0001, DpadDown 0x0002, DpadLeft 0x0004, DpadRight 0x0008, Start 0x0010, Back 0x0020, L3 0x0040, R3 0x0080, LB 0x0100, RB 0x0200, Guide 0x0400, A 0x1000, B 0x2000, X 0x4000, Y 0x8000
  - `pub struct Mapper { … }` with `pub fn new(profile: Profile, mode: PadMode, stadia: StadiaAction) -> Self`, `pub fn set(&mut self, profile: Profile, mode: PadMode, stadia: StadiaAction)`, `pub fn map(&mut self, s: &PadState) -> XOut`, `pub fn reset(&mut self)` (clears smoothing history; called on reconnect)
- Rules:
  - Default physical→Xbox: A B X Y LB RB L3 R3, OPTIONS→Back, MENU→Start, STADIA→Guide, D-pad→D-pad; ASSISTANT, CAPTURE, L2_CLICK, R2_CLICK → nothing. `remap` overrides by flag name.
  - Sticks: centre = (raw − 127.5)/127.5, Y negated (Xbox up-positive); radial dead zone `dz = deadzone_pct/100`; `m' = ((m − dz)/(1 − dz)).clamp(0,1)`; curve Linear `m'`, Smooth `m'²`, Aggressive `√m'`; output `dir · m'' · 32767`, rounded, clamped to `i16`.
  - Triggers: hair → `255` if `raw > start_pct·255/100` else `0`; otherwise `((raw/255 − s)/(e − s)).clamp(0,1)·255`.
  - Basic mode only: per-axis hysteresis — keep previous raw value unless `|new − prev| ≥ 2` or new ∈ {0, 255}. Competitive: raw values pass straight through.

- [ ] **Step 1: failing tests**

```rust
#[test] fn default_buttons() { let mut m = mapper(PadMode::Competitive); let o = m.map(&st(Buttons::A|Buttons::MENU|Buttons::STADIA|Buttons::CAPTURE)); assert_eq!(o.buttons, 0x1000|0x0010|0x0400); }
#[test] fn remap_and_disable() { let mut p = Profile::default(); p.remap.insert("A".into(), Target::Xbox(XButton::B)); p.remap.insert("Y".into(), Target::Disabled); let mut m = Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold); assert_eq!(m.map(&st(Buttons::A|Buttons::Y)).buttons, 0x2000); }
#[test] fn centre_is_zero_and_extremes_full() { let mut m = mapper(PadMode::Competitive); let o = m.map(&sticks(128,128)); assert_eq!((o.lx,o.ly),(0,0)); let o = m.map(&sticks(255,0)); assert!(o.lx > 23000 && o.ly > 23000); /* diagonal, normalised */ let o = m.map(&sticks(255,128)); assert_eq!(o.lx, 32767); }
#[test] fn deadzone() { let mut m = mapper(PadMode::Competitive); /* default 8% */ assert_eq!(m.map(&sticks(135,128)).lx, 0); assert!(m.map(&sticks(150,128)).lx > 0); }
#[test] fn curves_order() { let v = |c| { let mut p = Profile::default(); p.left.curve = c; p.left.deadzone_pct = 0; Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold).map(&sticks(192,128)).lx }; assert!(v(Curve::Aggressive) > v(Curve::Linear) && v(Curve::Linear) > v(Curve::Smooth)); }
#[test] fn triggers() { let mut m = mapper(PadMode::Competitive); assert_eq!(m.map(&trig(5)).lt, 0); assert_eq!(m.map(&trig(250)).lt, 255); let mut p = Profile::default(); p.triggers.hair = true; let mut h = Mapper::new(p, PadMode::Competitive, StadiaAction::GuideHold); assert_eq!(h.map(&trig(20)).lt, 255); }
#[test] fn basic_hysteresis() { let mut m = mapper(PadMode::Basic); let a = m.map(&sticks(200,128)).lx; assert_eq!(m.map(&sticks(201,128)).lx, a); assert_ne!(m.map(&sticks(203,128)).lx, a); m.reset(); }
#[test] fn competitive_no_hysteresis() { let mut m = mapper(PadMode::Competitive); let a = m.map(&sticks(200,128)).lx; assert_ne!(m.map(&sticks(201,128)).lx, a); }
```
(`st`, `sticks`, `trig`, `mapper`: test helpers building `PadState` with neutral defaults.)

- [ ] **Step 2:** run → FAIL · **Step 3:** implement · **Step 4:** run → PASS
- [ ] **Step 5:** commit `feat(core): button remap, radial dead zone, curves, triggers`

### Task 4: Special-button state machine

**Files:** Create `crates/pad-core/src/special.rs`

**Interfaces:**
- Consumes: `PadState`, `Buttons`; `AssistantAction`, `CaptureAction`, `StadiaAction`
- Produces: `#[derive(Debug, PartialEq, Clone, Copy)] pub enum Action { OpenGemini, Screenshot, ToggleRecording, OpenCompanion }`; `pub struct Special` with `pub fn new(a: AssistantAction, c: CaptureAction, s: StadiaAction) -> Self`, `pub fn set(&mut self, …same…)`, `pub fn update(&mut self, s: &PadState, now_ms: u64) -> Vec<Action>`, `pub fn next_deadline(&self) -> Option<u64>` (when a held button will cross its threshold, so the bridge can wake without new reports)
- Rules: Assistant press edge → `OpenGemini` (if Gemini). Capture: release before 500 ms → `Screenshot`; held to 500 ms → `ToggleRecording` once, release then does nothing. Stadia (GuideHold): held to 1000 ms → `OpenCompanion` once. Off/GuideOnly → no actions.

- [ ] **Step 1: failing tests**

```rust
#[test] fn assistant_edge_once() { let mut s = sp(); assert_eq!(s.update(&b(Buttons::ASSISTANT),0), vec![Action::OpenGemini]); assert!(s.update(&b(Buttons::ASSISTANT),10).is_empty()); }
#[test] fn capture_tap() { let mut s = sp(); s.update(&b(Buttons::CAPTURE),0); assert_eq!(s.update(&b(Buttons::empty()),200), vec![Action::Screenshot]); }
#[test] fn capture_hold() { let mut s = sp(); s.update(&b(Buttons::CAPTURE),0); assert_eq!(s.next_deadline(), Some(500)); assert_eq!(s.update(&b(Buttons::CAPTURE),500), vec![Action::ToggleRecording]); assert!(s.update(&b(Buttons::empty()),900).is_empty()); }
#[test] fn stadia_hold() { let mut s = sp(); s.update(&b(Buttons::STADIA),0); assert!(s.update(&b(Buttons::STADIA),999).is_empty()); assert_eq!(s.update(&b(Buttons::STADIA),1000), vec![Action::OpenCompanion]); }
#[test] fn off_does_nothing() { let mut s = Special::new(AssistantAction::Off, CaptureAction::Off, StadiaAction::GuideOnly); assert!(s.update(&b(Buttons::ASSISTANT|Buttons::CAPTURE|Buttons::STADIA),0).is_empty()); assert!(s.update(&b(Buttons::empty()),2000).is_empty()); }
```

- [ ] **Step 2:** FAIL · **Step 3:** implement · **Step 4:** PASS · **Step 5:** commit `feat(core): tap/hold actions for Assistant, Capture and Stadia buttons`

### Task 5: Latency/rate stats

**Files:** Create `crates/pad-core/src/stats.rs`

**Interfaces:**
- Produces: `pub struct Stats` with `pub fn new(window: usize /*1024*/) -> Self`, `pub fn record(&mut self, arrival_us: u64, done_us: u64)`, `pub fn snapshot(&self) -> StatsSnapshot`, `pub fn clear(&mut self)`; `#[derive(Serialize, Clone, Copy, Default, Debug, PartialEq)] pub struct StatsSnapshot { pub rate_hz: f32, pub interval_p50_ms: f32, pub interval_p99_ms: f32, pub interval_max_ms: f32, pub proc_p99_us: u32, pub samples: u32 }` — ring buffer; rate = (n−1)/(span seconds); percentiles by sort of a copy (snapshot is ~1 Hz, cheap).

- [ ] **Step 1: failing tests**

```rust
#[test] fn steady_125hz() { let mut s = Stats::new(1024); for i in 0..500u64 { s.record(i*8000, i*8000+150); } let x = s.snapshot(); assert!((x.rate_hz-125.0).abs()<0.5); assert!((x.interval_p50_ms-8.0).abs()<0.01); assert_eq!(x.proc_p99_us,150); }
#[test] fn outlier_shows_in_max_not_p50() { let mut s = Stats::new(1024); let mut t=0; for i in 0..200 { t += if i==100 {30000} else {8000}; s.record(t,t+10); } let x=s.snapshot(); assert!((x.interval_max_ms-30.0).abs()<0.01); assert!((x.interval_p50_ms-8.0).abs()<0.01); }
#[test] fn empty_is_zero() { assert_eq!(Stats::new(8).snapshot(), StatsSnapshot::default()); }
```

- [ ] **Step 2:** FAIL · **Step 3:** implement · **Step 4:** PASS · **Step 5:** commit `feat(core): rolling input rate and latency stats`

### Task 6: Windows glue + bridge (headless, hardware-verified)

**Files:**
- Create: `crates/pad-core/src/win/mod.rs`, `win/hid.rs`, `win/hidhide.rs`, `win/vigem.rs`, `win/log.rs`, `crates/pad-core/src/bridge.rs`, `crates/pad-core/examples/bridge_cli.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
  - `pub enum Transport { Usb, Bluetooth }`; `pub struct DeviceInfo { pub path: String, pub instance_id: String, pub transport: Transport }`
  - `pub fn choose_device(devs: &[DeviceInfo]) -> Option<&DeviceInfo>` (pure; USB first)
  - `win::hid`: `enumerate_stadia() -> Vec<DeviceInfo>` (gamepad collection, usage page 1 / usage 5, input len ≥ 10; regex `vid[_&](02)?18d1.*pid[_&]9400`), `HidDevice::open_shared(path) -> io::Result<HidDevice>` (sets `HidD_SetNumInputBuffers(512)`), `HidDevice::reader(queued: usize /*4*/) -> OverlappedReader` with `fn next(&mut self, timeout_ms: u32) -> io::Result<Option<(Vec<u8>, u64 /*arrival QPC µs*/)>>`, `HidDevice::write_output(&self, report: &[u8]) -> bool` (WriteFile, fallback HidD_SetOutputReport), `to_instance_id(path) -> String`
  - `win::hidhide`: `installed() -> bool`, `ensure_hidden(ids: &[String]) -> bool`, `unhide(ids: &[String])` — port of `StadiEm/StadiaPad/HidHideControl.cs`
  - `win::vigem`: `struct VirtualPad` with `connect() -> Result<VirtualPad, BridgeError>`, `submit(&mut self, o: &XOut)`, `take_rumble(&self) -> Option<(u8,u8)>` (latest notification via a channel)
  - `pub enum BridgeError { VigemMissing, HidHideMissing, Io(String) }`
  - `bridge::Bridge::start(settings: Settings) -> BridgeHandle`; `BridgeHandle { pub fn update_settings(&self, s: Settings), pub fn status(&self) -> Status, pub fn subscribe(&self, tx: Sender<BridgeEvent>), pub fn unsubscribe(&self), pub fn stop(self) }`
  - `#[derive(Serialize, Clone)] pub struct Status { pub connection: Option<Transport>, pub battery: Option<u8>, pub charging: bool, pub vigem_ok: bool, pub hidhide_ok: bool, pub ble_low_latency: LowLatencyState, pub rate_override_available: bool, pub other_mappers: Vec<String> }`; `pub enum LowLatencyState { Unsupported, Off, Active }` (filled in Task 7; Off until then)
  - `pub enum BridgeEvent { Pad { state: PadState, out: XOut }, Stats(StatsSnapshot), Status(Status), Action(Action) }` — `Pad` throttled to ≤ 60/s, only sent while subscribed.
- Bridge loop: discovery every 1 s (sleep) while disconnected; on connect: hide (if enabled), create `VirtualPad`, `Mapper::reset`, read loop with `next(timeout = special.next_deadline() or 1000)`; each report → parse → special → mapper → `submit` → `stats.record`; rumble scaled by `rumble_pct`, deduped; every 1 s re-check `choose_device` and switch if a USB pad appeared while on BLE; on disconnect: zero rumble, drop pad. `stop()` un-hides.

- [ ] **Step 1: failing unit tests (in `bridge.rs`)**

```rust
#[test] fn prefers_usb() { let d = [dev("bt", Transport::Bluetooth), dev("usb", Transport::Usb)]; assert_eq!(choose_device(&d).unwrap().path, "usb"); }
#[test] fn none_when_empty() { assert!(choose_device(&[]).is_none()); }
#[test] fn instance_id() { assert_eq!(to_instance_id(r"\\?\hid#vid_18d1&pid_9400&mi_01#7&abc&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"), r"HID\VID_18D1&PID_9400&MI_01\7&ABC&0&0000"); }
```
- [ ] **Step 2:** FAIL · **Step 3:** implement glue + bridge · **Step 4:** `cargo test -p pad-core` PASS
- [ ] **Step 5: hardware check.** Stop old `StadiaPad.exe`. Run `cargo run -p pad-core --example bridge_cli --release` (prints status + stats every second). Expected over USB: `connection=Usb`, rate ≈ 125, `proc_p99_us < 1000`; XInput probe shows exactly one controller following the sticks; XInput rumble probe produces no write error. Switch to Bluetooth: virtual pad recreated, still one controller. Ctrl+C: pad un-hidden (`HidHideCLI --dev-list` no longer lists it).
- [ ] **Step 6:** commit `feat(core): shared HID bridge to virtual Xbox 360 pad with hot-plug`

### Task 7: Competitive mode, BLE low latency, battery, polling-rate detection

**Files:** Create `win/perf.rs`, `win/ble.rs`, `win/rate.rs`; modify `bridge.rs`

**Interfaces:**
- Produces:
  - `win::perf::CompetitiveGuard::enter() -> CompetitiveGuard` (MMCSS `AvSetMmThreadCharacteristicsW("Games")` + `AvSetMmThreadPriority(AVRT_PRIORITY_HIGH)` on the read thread, `timeBeginPeriod(1)`; `Drop` reverts). Basic: no guard.
  - `win::ble::LowLatency::request(bt_addr: u64) -> Result<LowLatency, LowLatencyState>` — `BluetoothLEDevice::FromBluetoothAddressAsync`, `RequestPreferredConnectionParameters(BluetoothLEPreferredConnectionParameters::ThroughputOptimized())`; holds the request object; `Drop` closes it. `Unsupported` if the API is missing (`ApiInformation::IsMethodPresent`).
  - `win::ble::battery(bt_addr: u64) -> Option<u8>` (GATT 0x180F / 0x2A19 read; bridge refreshes every 60 s and on connect).
  - `win::rate::override_driver_present() -> bool` (registry key from Global Constraints).
  - BT address parsed from the BLE instance id (`..._CA6FFE0A918E\...` → `0xCA6FFE0A918E`): `pub fn bt_addr_from_path(path: &str) -> Option<u64>`.
  - Status fields `ble_low_latency`, `battery`, `charging` (USB → `charging=true`, battery `None`), `rate_override_available` now real.

- [ ] **Step 1: failing test** `#[test] fn bt_addr() { assert_eq!(bt_addr_from_path(r"\\?\hid#{00001812-0000-1000-8000-00805f9b34fb}_dev_vid&0218d1_pid&9400_rev&0100_ca6ffe0a918e#9&17c8ad2d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"), Some(0xCA6FFE0A918E)); assert_eq!(bt_addr_from_path(r"\\?\hid#vid_18d1&pid_9400&mi_01#7&abc#{x}"), None); }`
- [ ] **Step 2:** FAIL · **Step 3:** implement and wire: mode change applies live (guard/low-latency created or dropped on settings update) · **Step 4:** PASS
- [ ] **Step 5: hardware check (Bluetooth).** `bridge_cli` with Basic, then Competitive; record `interval_p99_ms` and `rate_hz` 15 s each while moving sticks. Expected: `ble_low_latency=Active` in Competitive; p99 lower than Basic (baseline measured 19.3 ms p90); battery shows a percentage. Record numbers in the commit message.
- [ ] **Step 6:** commit `feat(core): competitive mode, BLE low-latency request, battery`

### Task 8: Special-button actions

**Files:** Create `win/actions.rs`; modify `bridge.rs` (run actions off the input thread via a small worker channel)

**Interfaces:**
- Produces: `pub fn run(action: Action)` — `OpenGemini`: if a window titled containing "Gemini" exists (`EnumWindows`), focus it; else launch `msedge.exe --app=https://gemini.google.com/app --window-size=480,720`; fall back to default browser via `ShellExecuteW`. `Screenshot`: `SendInput` Win+Alt+PrtScn. `ToggleRecording`: `SendInput` Win+Alt+R. `OpenCompanion`: emitted as `BridgeEvent::Action`, handled by the app (Task 9). `pub fn game_bar_capture_enabled() -> bool` (`HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\AppCaptureEnabled` ≠ 0).
- [ ] **Step 1: hardware check** with `bridge_cli`: Assistant opens/focuses Gemini; Capture tap → new PNG in `%USERPROFILE%\Videos\Captures`; hold → recording starts, hold again → stops (MP4 appears); Assistant/Capture never reach the virtual pad (XInput probe shows no button bits).
- [ ] **Step 2:** commit `feat(core): Gemini, screenshot and recording actions`

### Task 9: Tauri shell — tray, lifecycle, IPC

**Files:** Create `app/src-tauri/{Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/*}`, `app/src-tauri/src/{main.rs, tray.rs, ipc.rs, autostart.rs}`; `app/ui/index.html` placeholder

**Interfaces:**
- Consumes: `BridgeHandle`, `Settings`, `Status`, `BridgeEvent`, `Action::OpenCompanion`
- Produces (Tauri commands): `get_settings() -> Settings`, `set_settings(s: Settings)` (saves, `update_settings`, applies `start_with_windows` via `HKCU\…\Run` value `PadCompanion`), `get_status() -> Status`, `test_rumble()`, `window_minimize/maximize/close()`. Events to UI: `"pad"`, `"stats"`, `"status"`.
- Window `main`: frameless (`decorations: false`), 1280×800, min 1100×700, created by `open_window()`; on `CloseRequested` → `destroy()` + `bridge.unsubscribe()`. Started with `--background` (autostart) → no window. Single instance: second launch calls `open_window()`. Tray: "Open Pad Companion", status line, "Start with Windows" check, "Quit" (stop bridge → un-hide → exit).
- [ ] **Step 1: failing test** in `ipc.rs`: `#[test] fn status_from_vigem_missing() { let s = status_for_error(&BridgeError::VigemMissing); assert!(!s.vigem_ok); assert!(s.connection.is_none()); }`
- [ ] **Step 2:** FAIL · **Step 3:** implement · **Step 4:** `cargo test` PASS
- [ ] **Step 5: lifecycle check.** `cargo tauri build`; run exe; open/close window 10×; after closing: no `msedgewebview2.exe` child of our process remains, private working set < 30 MB, controller keeps working throughout.
- [ ] **Step 6:** commit `feat(app): tray app with on-demand window and IPC`

### Task 10: UI — shell, theme, Home

**Files:** Create `app/ui/{index.html, css/tokens.css, css/app.css, js/theme.js, js/home.js, js/ipc.js, js/pad.js}`, `app/ui/assets/{fonts/RobotoFlex.woff2, fonts/MaterialSymbolsRounded.woff2, stadia.svg, gemini-light.svg}`

**Interfaces:**
- Consumes: commands/events from Task 9.
- Produces: `pad.js` `renderPad(svgEl, colors)` + `applyPadState(svgEl, state)` (lights buttons, moves sticks ±8 px, fills triggers), reused by Task 11; `theme.js` `applyScheme(color: "JustBlack"|"ClearlyWhite"|"Wasabi"|"NightBlue")` with the four tonal sets from `companion-v3.html`.
- Port from `.superpowers/brainstorm/1048-1793411586/content/companion-v3.html`: title bar (drag region, window buttons → commands), ambient glow, Home device card, palette pop-over with diffusion (`@property --r` mask), connection pill driven by `status.connection` (Bluetooth / USB cable / "Not connected" + hint "Plug in or hold Stadia + Y to pair"), battery chip (`charging` → "Charging"), click pad → device view. Palette choice saved to `pad_color`. Missing ViGEm/HidHide → banner with install link.
- [ ] **Step 1: check** in app: colour change persists across restart; pill flips within 2 s when unplugging/reconnecting over BT; buttons light live; `prefers-reduced-motion` disables float/blob animations.
- [ ] **Step 2:** commit `feat(ui): Material You shell and live Home card`

### Task 11: UI — device pages

**Files:** Create `app/ui/js/{device.js, pages/mapping.js, pages/special.js, pages/performance.js, pages/sticks.js, pages/rumble.js, pages/profiles.js, pages/other.js}`, `app/ui/css/pages.css`

- Every control reads from `get_settings()` and writes through `set_settings()` (debounced 150 ms).
- **Mapping:** hotspots around the live pad; clicking one opens a menu of `XButton` targets + Disabled; "Restore default".
- **Special:** three panels per mockup bound to `assistant`/`capture`/`stadia`; Capture panel warns if Game Bar capture is off.
- **Performance:** polling segmented 125/250/500/1000 (EXP badges; >125 disabled with reason text when `!rate_override_available` or on Bluetooth); Basic/Competitive cards; "Force Bluetooth Low-Latency Mode" switch (disabled with note unless Bluetooth and not `Unsupported`); live stats panel (rate, p50/p99/max interval, processing p99 in µs).
- **Sticks & triggers:** dead zone sliders with live dot preview from `pad` events, curve segmented, trigger start/end, hair trigger.
- **Rumble:** strength slider + "Test" (`test_rumble`; note "USB only" if Bluetooth rumble write fails).
- **Profiles:** list, create (copy of current), rename, delete (not Default), activate.
- **Other:** start with Windows, hide pad from games, open log folder, version.
- [ ] **Step 1: check** each page against the real pad; settings survive restart; switching profile changes mapping live (XInput probe).
- [ ] **Step 2:** commit `feat(ui): mapping, special buttons, performance, sticks, rumble, profiles`

### Task 12: Packaging

**Files:** modify `tauri.conf.json` (NSIS bundle, product name "Pad Companion", per-user install), add `app/src-tauri/installer/` hooks; `README.md`

- Installer checks for ViGEmBus and HidHide; if missing, offers to download the official installers (Nefarius GitHub releases) — no bundled drivers.
- [ ] **Step 1: check** fresh install on this PC (after uninstalling old StadiaPad autostart), run, uninstall leaves no autostart entry and the pad un-hidden.
- [ ] **Step 2:** commit `build: NSIS installer with driver checks`
