# Rumble over Bluetooth: why Pad Companion can't do it on Windows

Investigated 2026-11-01 on a Stadia controller with the Bluetooth-mode firmware (`337784`), Windows 11.
Re-run the read-only checks with `bridge_cli --hidcaps` and `bridge_cli --gatt`.

## The pad can do it; Windows won't pass it on

- Google's Linux driver (`hid-google-stadiaff`) was tested with rumble over wired **and** wireless, so
  the Bluetooth firmware accepts rumble.
- Over Bluetooth the pad declares the same rumble output as over USB: report `0x05`, two 16-bit motors,
  5 bytes. Pad Companion builds exactly that.
- Windows' Bluetooth HID driver (`hidbthle.inf`) rejects every rumble write with error 87 ("The parameter
  is incorrect"), through both `WriteFile` and `HidD_SetOutputReport`, at any length. reWASD's developer
  reached the same point in Jan 2023; SDL disables Stadia rumble on Bluetooth.

## Every way around the driver is closed

| Route | Result |
|---|---|
| WinRT GATT (`GattDeviceService`) on HID service `0x1812` | Characteristics query: `AccessDenied` |
| Win32 GATT (`BluetoothGATT*`) on the LE device handle | Lists the HID service and its Report characteristics (`0x003F` notify = input, `0x0043` write = rumble), but every value/descriptor access fails "Incorrect function" (value access needs a service handle) |
| HID service node's own GATT handle, on `hidbthle.inf` | Open fails "device not functioning" |
| Same node moved onto Windows' generic GATT driver (`bthleenum.inf` / UmPass, the driver the pad's battery service uses) | Opens only with no access rights; reads and writes of the Report characteristics: **Access denied, also as administrator**. The battery node opens read/write as a normal user. |

The Bluetooth stack itself reserves HID-over-GATT reports for Windows' HID driver. Only a custom,
Microsoft-signed kernel driver could get past that, which is out of scope for this app. The driver swap
was reverted on the test machine (node back on `hidbthle.inf`, pad working).

## What the app does instead

Each session probes rumble once with an all-motors-off report. When Windows refuses it (Bluetooth), rumble
is skipped for that session and `Status.rumble = false`; the Rumble page says vibration needs the cable.
