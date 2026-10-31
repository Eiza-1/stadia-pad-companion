# Pad Companion

A light, good-looking Windows companion for the Google Stadia controller. Plug it in or pair it over
Bluetooth and every game, Steam, Epic and EA launcher sees one Xbox 360 controller.

## What it does

- **Works over USB and Bluetooth.** If both are connected, USB wins, so there is only ever one controller.
- **No double input.** The real pad is hidden from games with HidHide; it's opened shared, so no app gets locked out.
- **Special buttons with real jobs.** Assistant opens Gemini. Capture takes a screenshot (tap) or starts/stops a
  recording (hold). Hold Stadia for 1 s to open Pad Companion over any game.
- **Performance modes.** Basic for everyday play. Competitive adds no smoothing, a game-priority input thread, and
  Force Bluetooth Low-Latency Mode (Windows 11). Live report-rate and latency read-outs show what you actually get.
- **Button mapping, dead zones, response curves, trigger ranges, hair trigger, rumble strength, profiles.**
- **Costs nothing in the background.** A Rust core with no busy loops. The window is destroyed when you close it,
  so its WebView2 processes exit. In testing: about 8 MB private memory and 0 CPU while idle.

## Requirements

- Windows 10 1903 or later (Force Bluetooth Low-Latency Mode needs Windows 11)
- [ViGEmBus](https://github.com/nefarius/ViGEmBus/releases/latest): required, so games can see the Xbox controller
- [HidHide](https://github.com/nefarius/HidHide/releases/latest): recommended, so games don't also see the Stadia pad

The installer checks for both and offers the official download pages. Drivers are never bundled.

## Honest notes

- **Polling rate.** The pad reports at 125 Hz over USB. Faster USB rates need a polling-rate override driver
  (hidusbf). Pad Companion unlocks those options only when that driver is installed and always shows the measured
  rate. Over Bluetooth the radio link sets the pace (about 7.5 ms at best), so higher rates are not offered.
- **Battery.** The pad's report rate doesn't change between modes, so pad battery use is about the same. Competitive
  uses a little more PC power.
- **Capture.** Screenshots and recordings use Xbox Game Bar. If Game Bar captures are turned off, a tap saves a
  full-screen screenshot to Pictures\Screenshots instead, and recording needs Game Bar turned back on.

## Pairing over Bluetooth

Hold **Stadia + Y** for 2 seconds until the light blinks, then add it in Windows Settings › Bluetooth & devices. If
Windows pairs it but Pad Companion says *Not connected*, remove the device and pair again: Windows sometimes skips the
controller service on the first try.

## Build from source

Requires Rust (MSVC toolchain), the Visual Studio C++ Build Tools and the Tauri CLI (`cargo install tauri-cli`).

```sh
cargo test --workspace          # core logic tests
sh scripts/check-ui.sh          # UI module syntax check
cd app/src-tauri && cargo tauri build   # installer in target/release/bundle/nsis
```

Settings and the log live in `%LOCALAPPDATA%\PadCompanion`.
