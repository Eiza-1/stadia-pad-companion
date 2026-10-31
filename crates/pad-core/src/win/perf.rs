//! Competitive mode: the input thread joins MMCSS "Games" at high priority and the system timer
//! runs at 1 ms. Created on the read thread while a pad is connected in Competitive; Drop reverts.

use crate::log;
use windows::core::w;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority, AVRT_PRIORITY_HIGH,
};

pub struct CompetitiveGuard {
    mmcss: Option<HANDLE>,
}

impl CompetitiveGuard {
    pub fn enter() -> CompetitiveGuard {
        let mut task = 0u32;
        let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Games"), &mut task) }.ok();
        match mmcss {
            Some(h) => unsafe {
                let _ = AvSetMmThreadPriority(h, AVRT_PRIORITY_HIGH);
            },
            None => log!("MMCSS Games registration failed"),
        }
        unsafe {
            timeBeginPeriod(1);
        }
        log!("competitive mode on");
        CompetitiveGuard { mmcss }
    }
}

impl Drop for CompetitiveGuard {
    fn drop(&mut self) {
        unsafe {
            timeEndPeriod(1);
            if let Some(h) = self.mmcss.take() {
                let _ = AvRevertMmThreadCharacteristics(h);
            }
        }
        log!("competitive mode off");
    }
}
