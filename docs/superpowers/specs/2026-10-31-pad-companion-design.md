# Pad Companion for Stadia Controller — Design

Date: 2026-10-31 · Status: approved to build ("that's all, you can start building")

## Intent

A public, downloadable Windows companion app for the Google Stadia controller. It makes the pad
"just work" as an Xbox 360 controller in every game and launcher (Steam, Epic, EA…) over **USB or
Bluetooth**, and wraps that in a polished Material You app modelled on premium mouse software
(reference: ATK Gear / VXE R1SE+ screenshots supplied by the user).

It replaces the earlier C# prototype `StadiEm\StadiaPad`, which proved the approach on the user's PC
(shared HID access, HidHide hiding, ViGEm Xbox 360 pad, WriteFile rumble, USB + BLE detection).

## Goals

1. Instant: plug in or connect over Bluetooth and it works, no setup screens.
2. Feather-light in the background, even on low-end PCs (Rust core, window costs nothing when closed).
3. Top-tier input: minimal added latency, no dropped input, honest live measurements.
4. Gorgeous, fluid Material You UI (Roboto Flex, Material Symbols, dynamic colour, spring motion).
5. Real jobs for the special buttons: Assistant → Gemini, Capture → screenshot / record.

## Non-goals (v1)

- macOS / Linux. Non-Stadia controllers. Cloud accounts or sync.
- Shipping a USB overclock driver (see Polling rate).
- DualShock/PlayStation emulation (Xbox 360 only).

## Anti-goals (failure even if it "works")

- Games or Steam seeing two controllers, or a ghost controller with no pad connected.
- Locking other apps out of the pad (the StadiEm "exclusive mode" failure).
- A background process that costs noticeable CPU/RAM, or wakes the CPU when idle.
- Settings labelled as doing something they don't (e.g. a "1000 Hz" button that changes nothing).

## Measured facts (user's pad, 2026-10-31)

- USB: 125 reports/s, steady 8.0 ms (p10 7.97, p90 8.02).
- Bluetooth LE: ~126 reports/s average but bursty: p10 0.55 ms, median 8.7 ms, p90 19.3 ms, max 29 ms.
- Input report ID 0x03 has the same layout over USB and BLE (see report map below).
- Rumble: `HidD_SetOutputReport` fails (error 31); `WriteFile` of report 0x05 (5 bytes) works over USB.
- BLE exposes Battery Service 0x180F. Pairing can fail to enumerate the HID service; re-pairing fixes it.

Report map (ID 0x03): `[1]` hat 0–7 clockwise from up, 8 = released · `[2]` 0x80 R3, 0x40 Options,
0x20 Menu, 0x10 Stadia, 0x08 R2-click, 0x04 L2-click, 0x02 Assistant, 0x01 Capture · `[3]` 0x40 A,
0x20 B, 0x10 X, 0x08 Y, 0x04 LB, 0x02 RB, 0x01 L3 · `[4..7]` LX LY RX RY (0–255, Y down-positive) ·
`[8]` L2 · `[9]` R2. Rumble ID 0x05: large motor LE16, small motor LE16.

## Architecture (decided: Rust)

One Rust executable built with **Tauri 2**.

- **Core (always running, pure Rust):** device discovery and hot-plug, HID reading, mapping, ViGEm
  Xbox 360 target, HidHide control, rumble, battery, special-button actions, performance modes,
  stats. Runs on dedicated threads; idle cost ≈ 0 (blocking waits, no polling loops faster than 1 s
  for discovery, and discovery uses device-arrival notifications where possible).
- **Tray icon** while running. Settings window is created on demand and **destroyed on close** so the
  WebView2 processes exit. Closing the window never stops the controller bridge; "Quit" in the tray does.
- **UI:** plain HTML/CSS/JS (no framework, no bundler) served by Tauri, talking to the core with
  Tauri commands and events. Fonts (Roboto Flex, Material Symbols Rounded) bundled locally, offline.
- **Settings:** JSON in `%LOCALAPPDATA%\PadCompanion\settings.json`; log in the same folder (size-capped).
- Dependencies on the user's PC: ViGEmBus and HidHide drivers (detected; the app explains and links
  to installers if missing). Single instance enforced.

## Input pipeline ("Competitive" quality in all modes where it costs nothing)

- Open the pad **shared** (`FILE_SHARE_READ|WRITE`), never exclusive.
- `HidD_SetNumInputBuffers(512)` so bursts are never dropped by the HID class driver.
- **Overlapped I/O with several reads always queued** (4), completed via an I/O completion port or
  wait, so a report is never lost between reads. (Stadia reports carry no sequence number, so drops
  are prevented, not counted; the UI shows interval outliers instead.)
- Each report is forwarded to ViGEm immediately on arrival (no batching, no timers).
- Separate write handle for rumble so output never waits on input.
- Stats: reports/s, interval histogram (p50/p99/max), app processing time (arrival → ViGEm submit,
  target < 1 ms p99). Shown live on the Performance page.
- If USB and Bluetooth are both present, use USB (one virtual pad only). Switching transport
  recreates the virtual pad; there is never more than one.
- On Quit, the pad is un-hidden so it behaves normally without the app.
- Warn if other mappers run (StadiaPad, StadiaBridge, StadiEm, DS4Windows, x360ce).

## Performance page

**Polling rate** segmented control: 125 · 250 · 500 · 1000 Hz, with 500/1000 badged EXPERIMENTAL and a
lag/battery warning. Live meter shows the rate actually achieved.

- 125 Hz is the pad's native rate and the default.
- Raising the USB rate needs a USB interrupt-interval override filter driver. **v1 does not ship
  one.** 250+ is enabled only when such a driver is detected; otherwise the buttons explain why
  and are disabled. Over Bluetooth all options above 125 are disabled with an explanation (link
  limit ≈ 7.5 ms). (Builder's call; revisit in a later version.)

**Pad mode** toggle:

- **Basic:** light stick smoothing (removes sub-dead-zone jitter), normal thread priority,
  Windows power management left alone. Default.
- **Competitive:** no smoothing; input thread registered with MMCSS "Games" at high priority;
  `timeBeginPeriod(1)` only while a pad is connected; USB selective suspend avoided for the pad;
  **Force Bluetooth Low-Latency Mode** (below). Battery: the pad's report rate doesn't change, so
  pad drain stays close to Basic; the cost is a little PC CPU/power, and low-latency BLE may cost
  the pad some extra battery — the UI shows an estimate and the live battery reading.

**Force Bluetooth Low-Latency Mode** (sub-toggle, on by default in Competitive, BLE only):
request `BluetoothLEPreferredConnectionParameters::ThroughputOptimized` (Windows 11) for the pad via
`BluetoothLEDevice::RequestPreferredConnectionParameters`, hold the request for the session, re-apply
on reconnect, release in Basic. Show the resulting interval/jitter in the live stats. If the OS
doesn't support it, the toggle is disabled with a note.

## Special buttons

- **Assistant → Gemini:** opens `https://gemini.google.com` in a small app-style window (Edge
  `--app=` mode) or focuses it if already open. Options: Gemini · Off.
- **Capture:** tap = screenshot, hold (≥ 0.5 s) = start/stop recording. v1 drives Windows Game Bar
  shortcuts (Win+Alt+PrtScn, Win+Alt+R) via `SendInput`, files land in Videos\Captures; the app
  detects if Game Bar capture is disabled and says how to enable it. Options: Tap+hold · Off.
- **Stadia button:** Xbox Guide in games; hold ≥ 1 s opens the Pad Companion window. Options:
  Guide+hold · Guide only.
- Assistant and Capture are **not** forwarded to the virtual pad.

## Other features

- Button remapping (any physical → any Xbox button / none), stick dead zone + response curve
  (Linear / Smooth / Aggressive), trigger start/end points and hair-trigger, rumble strength + test,
  profiles (named sets of the above; Default always exists), start with Windows, hide pad from games
  toggle, restore defaults.

## UI (from approved mockup `companion-v3.html`)

- Dark Material You; scheme re-seeded from the selected pad colour (Just Black, Clearly White,
  Wasabi, Night Blue); ambient blurred glow + faint Stadia mark background.
- Title bar: brand chip (Stadia logo + "Pad Companion"), Home, device tab, service ON chip, icon
  buttons, custom window controls (frameless window).
- **Home:** frosted device card; pad rendering; name; connection pill (Bluetooth / USB cable, live
  pulse; "Not connected" state); battery; palette icon → swatches pop out, hover **diffuses** the
  colour into the pad from the cursor and re-tints the app; click pad to enter.
- **Device view:** sidebar (device header, profile, Button mapping, Special buttons, Performance,
  Sticks & triggers, Rumble, Profiles, Other settings, Report a bug); large pad with labelled
  hotspots; pages animate with spring motion. Live pad highlights pressed buttons and moving sticks.
- Logos: user-supplied Stadia and Gemini SVGs used where the mockup uses them. Trademark caveat
  for public release was raised with the user; their call, deferred.

## Left to the builder

Module layout, crate choices (`windows`, `vigem-client`, `tauri`), IPC shape, smoothing filter,
hold timings, exact stat windows, tray menu wording, installer (Tauri NSIS bundle) details.
