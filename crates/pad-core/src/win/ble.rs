//! Bluetooth LE extras: Force Low-Latency (preferred connection parameters) and battery level.

use crate::bridge::LowLatencyState;
use crate::log;
use windows::core::h;
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

/// Holds a ThroughputOptimized (shortest interval, no slave latency) connection-parameter request
/// for the pad. Windows keeps it while this object lives; dropping it releases the request.
pub struct LowLatency {
    request: BluetoothLEPreferredConnectionParametersRequest,
    _device: BluetoothLEDevice,
}

impl LowLatency {
    pub fn request(bt_addr: u64) -> Result<LowLatency, LowLatencyState> {
        if !low_latency_supported() {
            return Err(LowLatencyState::Unsupported);
        }
        let attempt = || -> windows::core::Result<(BluetoothLEDevice, BluetoothLEPreferredConnectionParametersRequest)> {
            let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
            let params = BluetoothLEPreferredConnectionParameters::ThroughputOptimized()?;
            let request = device.RequestPreferredConnectionParameters(&params)?;
            Ok((device, request))
        };
        match attempt() {
            Ok((device, request)) => {
                let status = request.Status();
                log!("BLE low-latency request: {status:?}");
                if status.is_ok_and(|s| s == BluetoothLEPreferredConnectionParametersRequestStatus::Success) {
                    Ok(LowLatency { request, _device: device })
                } else {
                    let _ = request.Close();
                    Err(LowLatencyState::Off)
                }
            }
            Err(e) => {
                log!("BLE low-latency request failed: {e}");
                Err(LowLatencyState::Off)
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

/// Battery Service 0x180F / Battery Level 0x2A19, read uncached. `None` if unavailable.
pub fn battery(bt_addr: u64) -> Option<u8> {
    let read = || -> windows::core::Result<Option<u8>> {
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(bt_addr)?.join()?;
        let services = device
            .GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::Battery()?, BluetoothCacheMode::Uncached)?
            .join()?;
        if services.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        let Some(service) = services.Services()?.into_iter().next() else { return Ok(None) };
        let chars = service.GetCharacteristicsForUuidAsync(GattCharacteristicUuids::BatteryLevel()?)?.join()?;
        let Some(ch) = chars.Characteristics()?.into_iter().next() else { return Ok(None) };
        let value = ch.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?.join()?;
        if value.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        let level = DataReader::FromBuffer(&value.Value()?)?.ReadByte()?;
        let _ = service.Close();
        let _ = device.Close();
        Ok(Some(level.min(100)))
    };
    read().unwrap_or_else(|e| {
        log!("battery read failed: {e}");
        None
    })
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
