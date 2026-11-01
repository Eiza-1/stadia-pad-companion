//! Bluetooth LE extras: link parameters (read, and presets for the bench), battery level and firmware string.

use crate::log;
use windows::core::{h, GUID};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicUuids, GattCommunicationStatus, GattServiceUuids,
};
use windows::Devices::Bluetooth::{
    BluetoothCacheMode, BluetoothLEDevice, BluetoothLEPreferredConnectionParameters,
    BluetoothLEPreferredConnectionParametersRequest, BluetoothLEPreferredConnectionParametersRequestStatus,
};
use windows::Foundation::Metadata::ApiInformation;
use windows::Storage::Streams::DataReader;

/// `..._rev&0100_ca6ffe0a918e#...` → `0xCA6FFE0A918E`. `None` for USB paths.
pub fn bt_addr_from_path(path: &str) -> Option<u64> {
    let p = path.to_ascii_lowercase();
    if !p.contains("{00001812-") {
        return None;
    }
    let rev = p.find("_rev&")?;
    let tail = &p[rev + 5..];
    let addr = tail.split(['#', '\\']).next()?.split('_').nth(1)?;
    (addr.len() == 12).then(|| u64::from_str_radix(addr, 16).ok()).flatten()
}

/// Windows 11 API; absent on Windows 10, where the toggle is disabled with a note.
pub fn low_latency_supported() -> bool {
    ApiInformation::IsMethodPresent(
        h!("Windows.Devices.Bluetooth.BluetoothLEDevice"),
        h!("RequestPreferredConnectionParameters"),
    )
    .unwrap_or(false)
}

/// Holds a connection-parameter preset request while it lives. Used only by `bridge_cli --bench`: the
/// app makes no request, because no preset beat Windows' default link for the pad (see bridge.rs Perf).
pub struct LowLatency {
    request: BluetoothLEPreferredConnectionParametersRequest,
    _device: BluetoothLEDevice,
}

/// Windows' connection-parameter presets (it publishes no exact values for them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkPreset {
    Balanced,
    Throughput,
    Power,
}

/// The link the pad actually has right now: interval in ms, peripheral latency (events it may skip),
/// supervision timeout in ms. Windows 11 only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkParams {
    pub interval_ms: f32,
    pub latency: u16,
    pub timeout_ms: u32,
}

pub fn link_params(bt_addr: u64) -> Option<LinkParams> {
    let read = || -> windows::core::Result<LinkParams> {
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
        let p = device.GetConnectionParameters()?;
        let out = LinkParams {
            interval_ms: p.ConnectionInterval()? as f32 * 1.25,
            latency: p.ConnectionLatency()?,
            timeout_ms: p.LinkTimeout()? as u32 * 10,
        };
        let _ = device.Close();
        Ok(out)
    };
    read().ok()
}

impl LowLatency {
    pub fn request_preset(bt_addr: u64, preset: LinkPreset) -> Result<LowLatency, String> {
        if !low_latency_supported() {
            return Err("needs Windows 11".into());
        }
        let attempt = || -> windows::core::Result<(BluetoothLEDevice, BluetoothLEPreferredConnectionParametersRequest)> {
            let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
            let params = match preset {
                LinkPreset::Balanced => BluetoothLEPreferredConnectionParameters::Balanced()?,
                LinkPreset::Throughput => BluetoothLEPreferredConnectionParameters::ThroughputOptimized()?,
                LinkPreset::Power => BluetoothLEPreferredConnectionParameters::PowerOptimized()?,
            };
            let request = device.RequestPreferredConnectionParameters(&params)?;
            Ok((device, request))
        };
        match attempt() {
            Ok((device, request)) => {
                let status = request.Status();
                log!("BLE {preset:?} request: {status:?}");
                if status.as_ref().is_ok_and(|s| *s == BluetoothLEPreferredConnectionParametersRequestStatus::Success) {
                    Ok(LowLatency { request, _device: device })
                } else {
                    let _ = request.Close();
                    Err(format!("not granted ({status:?})"))
                }
            }
            Err(e) => {
                log!("BLE {preset:?} request failed: {e}");
                Err(e.to_string())
            }
        }
    }
}

impl Drop for LowLatency {
    fn drop(&mut self) {
        let _ = self.request.Close();
        log!("BLE low-latency released");
    }
}

/// Reads one GATT characteristic uncached. `None` if the service, characteristic or read is unavailable.
fn read_characteristic(bt_addr: u64, service: GUID, characteristic: GUID) -> windows::core::Result<Option<Vec<u8>>> {
    let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
    let services = device.GetGattServicesForUuidWithCacheModeAsync(service, BluetoothCacheMode::Uncached)?.join()?;
    if services.Status()? != GattCommunicationStatus::Success {
        return Ok(None);
    }
    let Some(service) = services.Services()?.into_iter().next() else { return Ok(None) };
    let chars = service.GetCharacteristicsForUuidAsync(characteristic)?.join()?;
    let Some(ch) = chars.Characteristics()?.into_iter().next() else { return Ok(None) };
    let value = ch.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?.join()?;
    let bytes = if value.Status()? == GattCommunicationStatus::Success {
        let buf = value.Value()?;
        let mut out = vec![0u8; buf.Length()? as usize];
        DataReader::FromBuffer(&buf)?.ReadBytes(&mut out)?;
        Some(out)
    } else {
        None
    };
    let _ = service.Close();
    let _ = device.Close();
    Ok(bytes)
}

/// Battery Service 0x180F / Battery Level 0x2A19. `None` if unavailable.
pub fn battery(bt_addr: u64) -> Option<u8> {
    let read = || read_characteristic(bt_addr, GattServiceUuids::Battery()?, GattCharacteristicUuids::BatteryLevel()?);
    match read() {
        Ok(v) => v.and_then(|b| b.first().map(|l| (*l).min(100))),
        Err(e) => {
            log!("battery read failed: {e}");
            None
        }
    }
}

/// Device Information 0x180A / Firmware Revision String 0x2A26. `None` if unavailable.
pub fn firmware_revision(bt_addr: u64) -> Option<String> {
    let read = || {
        read_characteristic(bt_addr, GattServiceUuids::DeviceInformation()?, GattCharacteristicUuids::FirmwareRevisionString()?)
    };
    match read() {
        Ok(v) => v.map(|b| String::from_utf8_lossy(&b).trim_end_matches('\0').trim().to_string()).filter(|s| !s.is_empty()),
        Err(e) => {
            log!("firmware read failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bt_addr() {
        assert_eq!(
            bt_addr_from_path(r"\\?\hid#{00001812-0000-1000-8000-00805f9b34fb}_dev_vid&0218d1_pid&9400_rev&0100_ca6ffe0a918e#9&17c8ad2d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"),
            Some(0xCA6FFE0A918E)
        );
        assert_eq!(bt_addr_from_path(r"\\?\hid#vid_18d1&pid_9400&mi_01#7&abc#{x}"), None);
    }
}

/// Diagnostics for `bridge_cli --gatt`: what the pad's HID-over-GATT service exposes to apps (status,
/// each characteristic's properties and its Report Reference: report id + type 1 in / 2 out / 3 feature).
pub fn hid_gatt_probe(bt_addr: u64) -> Vec<String> {
    use windows::Devices::Bluetooth::GenericAttributeProfile::GattCharacteristicProperties as P;
    let mut out = Vec::new();
    let run = |out: &mut Vec<String>| -> windows::core::Result<()> {
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
        let hid = GUID::from_u128(0x00001812_0000_1000_8000_00805f9b34fb);
        let sr = device.GetGattServicesForUuidWithCacheModeAsync(hid, BluetoothCacheMode::Uncached)?.join()?;
        out.push(format!("HID service query: {:?}, {} service(s)", sr.Status()?, sr.Services()?.Size()?));
        for svc in sr.Services()? {
            let cr = svc.GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?.join()?;
            out.push(format!("  characteristics query: {:?}", cr.Status()?));
            for ch in cr.Characteristics()? {
                let props = ch.CharacteristicProperties()?;
                let names: Vec<&str> = [(P::Read, "read"), (P::Write, "write"), (P::WriteWithoutResponse, "write-no-resp"), (P::Notify, "notify")]
                    .into_iter()
                    .filter(|(f, _)| props.contains(*f))
                    .map(|(_, n)| n)
                    .collect();
                let mut refs = String::new();
                if let Ok(dr) = ch.GetDescriptorsForUuidAsync(GUID::from_u128(0x00002908_0000_1000_8000_00805f9b34fb)).and_then(|a| a.join()) {
                    for d in dr.Descriptors()? {
                        if let Ok(v) = d.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached).and_then(|a| a.join()) {
                            if v.Status()? == GattCommunicationStatus::Success {
                                let buf = v.Value()?;
                                let mut b = vec![0u8; buf.Length()? as usize];
                                DataReader::FromBuffer(&buf)?.ReadBytes(&mut b)?;
                                refs = format!(" report-ref {b:02X?}");
                            } else {
                                refs = format!(" report-ref read {:?}", v.Status()?);
                            }
                        }
                    }
                }
                out.push(format!("    {:?} [{}]{refs}", ch.Uuid()?, names.join(",")));
            }
        }
        let _ = device.Close();
        Ok(())
    };
    if let Err(e) = run(&mut out) {
        out.push(format!("error: {e}"));
    }
    out
}
