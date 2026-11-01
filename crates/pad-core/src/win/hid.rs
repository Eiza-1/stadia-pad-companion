//! Win32 HID access. Every handle is opened SHARED so Steam, browsers and other apps are never locked out.

use crate::bridge::{to_instance_id, DeviceInfo, Transport};
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW, CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CR_SUCCESS,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetProductString, HidD_GetSerialNumberString,
    HIDD_ATTRIBUTES, HidP_GetValueCaps, HidP_Output, HIDP_VALUE_CAPS, HidD_GetPreparsedData, HidD_SetNumInputBuffers, HidD_SetOutputReport,
    HidP_GetCaps, HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn is_stadia(path: &str) -> bool {
    // USB: hid#vid_18d1&pid_9400...   Bluetooth LE: hid#{00001812-...}_dev_vid&0218d1_pid&9400...
    let p = path.to_ascii_lowercase();
    let vid = p.find("vid_18d1").or_else(|| p.find("vid&0218d1"));
    let pid = p.find("pid_9400").or_else(|| p.find("pid&9400"));
    matches!((vid, pid), (Some(v), Some(q)) if q > v)
}

/// Microseconds on the QPC clock (monotonic, sub-µs resolution).
pub fn now_us() -> u64 {
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    if f == 0 {
        return 0;
    }
    ((c as i128 * 1_000_000) / f as i128) as u64
}

fn hid_interface_paths() -> Vec<String> {
    let guid: GUID = unsafe { HidD_GetHidGuid() };
    let mut len = 0u32;
    unsafe {
        if CM_Get_Device_Interface_List_SizeW(&mut len, &guid, PCWSTR::null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
            || len == 0
        {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_Interface_ListW(&guid, PCWSTR::null(), &mut buf, CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
        {
            return Vec::new();
        }
        buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
    }
}

/// Every present Stadia gamepad collection (usage page 1, usage 5, input report ≥ 10 bytes).
pub fn enumerate_stadia() -> Vec<DeviceInfo> {
    hid_interface_paths()
        .into_iter()
        .filter(|p| is_stadia(p))
        .filter(|p| {
            HidDevice::open(p, false)
                .ok()
                .and_then(|d| d.caps())
                .is_some_and(|c| c.UsagePage == 0x01 && c.Usage == 0x05 && c.InputReportByteLength >= 10)
        })
        .map(|path| DeviceInfo {
            instance_id: to_instance_id(&path),
            transport: if path.to_ascii_lowercase().contains("{00001812-") { Transport::Bluetooth } else { Transport::Usb },
            path,
        })
        .collect()
}

pub struct HidDevice {
    handle: HANDLE,
    pub input_len: usize,
    pub output_len: usize,
}

unsafe impl Send for HidDevice {}
unsafe impl Sync for HidDevice {}

impl HidDevice {
    fn open(path: &str, overlapped: bool) -> io::Result<HidDevice> {
        let w = wide(path);
        let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { FILE_FLAGS_AND_ATTRIBUTES(0) };
        let handle = unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                flags,
                None,
            )
        }
        .map_err(io::Error::other)?;
        let mut d = HidDevice { handle, input_len: 0, output_len: 0 };
        if let Some(c) = d.caps() {
            d.input_len = c.InputReportByteLength as usize;
            d.output_len = c.OutputReportByteLength as usize;
        }
        Ok(d)
    }

    /// Shared read/write handle for overlapped input, with a 512-report kernel buffer so bursts are never dropped.
    pub fn open_shared(path: &str) -> io::Result<HidDevice> {
        let d = Self::open(path, true)?;
        unsafe {
            let _ = HidD_SetNumInputBuffers(d.handle, 512);
        }
        Ok(d)
    }

    /// Separate synchronous handle for rumble so output never waits on input.
    pub fn open_writer(path: &str) -> io::Result<HidDevice> {
        Self::open(path, false)
    }

    pub fn caps(&self) -> Option<HIDP_CAPS> {
        unsafe {
            let mut pp = PHIDP_PREPARSED_DATA::default();
            if !HidD_GetPreparsedData(self.handle, &mut pp) {
                return None;
            }
            let mut caps = HIDP_CAPS::default();
            let ok = HidP_GetCaps(pp, &mut caps) == HIDP_STATUS_SUCCESS;
            let _ = HidD_FreePreparsedData(pp);
            ok.then_some(caps)
        }
    }

    /// Output report via the interrupt OUT pipe (WriteFile). The Stadia pad stalls the control-pipe
    /// SET_REPORT used by HidD_SetOutputReport (error 31), which stays only as a fallback.
    pub fn write_output(&self, report: &[u8]) -> bool {
        unsafe {
            let mut written = 0u32;
            WriteFile(self.handle, Some(report), Some(&mut written), None).is_ok()
                || HidD_SetOutputReport(self.handle, report.as_ptr() as *const c_void, report.len() as u32)
        }
    }

    pub fn reader(&self, queued: usize) -> io::Result<OverlappedReader<'_>> {
        OverlappedReader::new(self, queued)
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

struct Slot {
    ov: OVERLAPPED,
    buf: Vec<u8>,
    pending: bool,
}

/// Keeps several reads queued at all times so no report waits for a read to be issued.
/// HID completes pending reads in issue order, so slots are consumed round-robin.
pub struct OverlappedReader<'a> {
    dev: &'a HidDevice,
    slots: Box<[Slot]>,
    next: usize,
}

impl<'a> OverlappedReader<'a> {
    fn new(dev: &'a HidDevice, queued: usize) -> io::Result<Self> {
        let len = dev.input_len.max(64);
        let mut slots = Vec::with_capacity(queued.max(1));
        for _ in 0..queued.max(1) {
            let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(io::Error::other)?;
            let ov = OVERLAPPED { hEvent: event, ..Default::default() };
            slots.push(Slot { ov, buf: vec![0; len], pending: false });
        }
        let mut r = OverlappedReader { dev, slots: slots.into_boxed_slice(), next: 0 };
        for i in 0..r.slots.len() {
            r.issue(i)?;
        }
        Ok(r)
    }

    fn issue(&mut self, i: usize) -> io::Result<()> {
        let s = &mut self.slots[i];
        let event = s.ov.hEvent;
        s.ov = OVERLAPPED { hEvent: event, ..Default::default() };
        let buf = unsafe { std::slice::from_raw_parts_mut(s.buf.as_mut_ptr(), s.buf.len()) };
        match unsafe { ReadFile(self.dev.handle, Some(buf), None, Some(&mut s.ov)) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {}
            Err(e) => return Err(io::Error::other(e)),
        }
        s.pending = true;
        Ok(())
    }

    /// Next report and its arrival time (QPC µs); `Ok(None)` on timeout. Errors mean the device is gone.
    pub fn next(&mut self, timeout_ms: u32) -> io::Result<Option<(Vec<u8>, u64)>> {
        let i = self.next;
        let w = unsafe { WaitForSingleObject(self.slots[i].ov.hEvent, timeout_ms) };
        if w == WAIT_TIMEOUT {
            return Ok(None);
        }
        if w != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        let arrival = now_us();
        let mut n = 0u32;
        let s = &mut self.slots[i];
        let res = unsafe { GetOverlappedResult(self.dev.handle, &s.ov, &mut n, false) };
        s.pending = false;
        res.map_err(io::Error::other)?;
        let data = s.buf[..n as usize].to_vec();
        self.issue(i)?;
        self.next = (i + 1) % self.slots.len();
        Ok(Some((data, arrival)))
    }
}

impl Drop for OverlappedReader<'_> {
    fn drop(&mut self) {
        unsafe {
            for s in self.slots.iter_mut() {
                if s.pending {
                    let _ = CancelIoEx(self.dev.handle, Some(&s.ov));
                    let mut n = 0u32;
                    // Wait for the cancel so the kernel no longer writes into this buffer.
                    let _ = GetOverlappedResult(self.dev.handle, &s.ov, &mut n, true);
                }
                let _ = CloseHandle(s.ov.hEvent);
            }
        }
    }
}

/// Cheap check (no handles opened): is any USB Stadia HID interface present?
pub fn usb_stadia_present() -> bool {
    hid_interface_paths().iter().any(|p| is_stadia(p) && !p.to_ascii_lowercase().contains("{00001812-"))
}

/// Instance ids of every present Stadia HID interface (no handles opened; cheap enough for a 1 s check).
pub fn stadia_instance_ids() -> Vec<String> {
    hid_interface_paths().iter().filter(|p| is_stadia(p)).map(|p| to_instance_id(p)).collect()
}

/// What the pad reports about itself: USB/BLE ids, the device release number and its strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    /// bcdDevice over USB, the DIS PnP "product version" over Bluetooth LE.
    pub version: u16,
    pub product: String,
    pub serial: String,
}

impl HidDevice {
    pub fn device_info(&self) -> Option<RawDeviceInfo> {
        let mut a = HIDD_ATTRIBUTES { Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
        unsafe {
            if !HidD_GetAttributes(self.handle, &mut a) {
                return None;
            }
        }
        let string = |f: unsafe fn(HANDLE, *mut c_void, u32) -> bool| {
            let mut buf = [0u16; 128];
            if unsafe { f(self.handle, buf.as_mut_ptr() as *mut c_void, (buf.len() * 2) as u32) } {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..n])
            } else {
                String::new()
            }
        };
        Some(RawDeviceInfo {
            vid: a.VendorID,
            pid: a.ProductID,
            version: a.VersionNumber,
            product: string(|h, b, l| unsafe { HidD_GetProductString(h, b, l) }),
            serial: string(|h, b, l| unsafe { HidD_GetSerialNumberString(h, b, l) }),
        })
    }
}

/// Opens a Stadia HID path just long enough to read its identity (no read/write access needed).
pub fn read_device_info(path: &str) -> Option<RawDeviceInfo> {
    HidDevice::open(path, false).ok()?.device_info()
}

/// Diagnostics for `bridge_cli --hidcaps`: every present Stadia HID interface with its top-level usage,
/// report lengths and output value fields (report id, usage, bits x count). Opens no read access.
pub fn describe_stadia_interfaces() -> Vec<String> {
    let mut out = Vec::new();
    for p in hid_interface_paths().into_iter().filter(|p| is_stadia(p)) {
        let Ok(d) = HidDevice::open(&p, false) else {
            out.push(format!("{p}
    (could not open: {})", io::Error::last_os_error()));
            continue;
        };
        let mut line = format!("{p}
");
        unsafe {
            let mut pp = PHIDP_PREPARSED_DATA::default();
            if HidD_GetPreparsedData(d.handle, &mut pp) {
                let mut c = HIDP_CAPS::default();
                if HidP_GetCaps(pp, &mut c) == HIDP_STATUS_SUCCESS {
                    line += &format!(
                        "    usage {:04X}:{:04X}  input {} B  output {} B  feature {} B  output value caps {}
",
                        c.UsagePage, c.Usage, c.InputReportByteLength, c.OutputReportByteLength, c.FeatureReportByteLength, c.NumberOutputValueCaps
                    );
                    let mut n = c.NumberOutputValueCaps;
                    if n > 0 {
                        let mut v = vec![HIDP_VALUE_CAPS::default(); n as usize];
                        if HidP_GetValueCaps(HidP_Output, v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                            for vc in &v[..n as usize] {
                                let usage = if vc.IsRange { vc.Anonymous.Range.UsageMin } else { vc.Anonymous.NotRange.Usage };
                                line += &format!(
                                    "      out id 0x{:02X}  usage {:04X}:{:04X}  {} bits x {}
",
                                    vc.ReportID, vc.UsagePage, usage, vc.BitSize, vc.ReportCount
                                );
                            }
                        }
                    }
                }
                let _ = HidD_FreePreparsedData(pp);
            }
        }
        out.push(line);
    }
    out
}

/// Diagnostics: tries a zero-rumble output (all motors off, harmless) both ways and reports each result.
pub fn try_output(path: &str, report: &[u8]) -> String {
    let Ok(d) = HidDevice::open(path, false) else { return format!("open failed: {}", io::Error::last_os_error()) };
    unsafe {
        let mut written = 0u32;
        let wf = match WriteFile(d.handle, Some(report), Some(&mut written), None) {
            Ok(()) => format!("WriteFile ok ({written} B)"),
            Err(e) => format!("WriteFile err {e}"),
        };
        let so = if HidD_SetOutputReport(d.handle, report.as_ptr() as *const c_void, report.len() as u32) {
            "SetOutputReport ok".to_string()
        } else {
            format!("SetOutputReport err {}", io::Error::last_os_error())
        };
        format!("{} B id 0x{:02X}: {wf}; {so}", report.len(), report.first().copied().unwrap_or(0))
    }
}
