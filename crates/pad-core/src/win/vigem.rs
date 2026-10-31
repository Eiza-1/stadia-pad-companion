//! One virtual Xbox 360 pad on ViGEmBus. Rumble requests from games arrive on a dedicated thread
//! and go straight to the caller's callback, so output never waits on the input loop.

use crate::bridge::BridgeError;
use crate::mapping::XOut;
use std::thread::JoinHandle;
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

pub struct VirtualPad {
    target: Option<Xbox360Wired<Client>>,
    notify: Option<JoinHandle<()>>,
    last: Option<XOut>,
}

impl VirtualPad {
    pub fn connect(mut on_rumble: impl FnMut(u8, u8) + Send + 'static) -> Result<VirtualPad, BridgeError> {
        let client = Client::connect().map_err(|e| match e {
            vigem_client::Error::BusNotFound => BridgeError::VigemMissing,
            e => BridgeError::Io(format!("ViGEm connect: {e}")),
        })?;
        let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        target.plugin().map_err(|e| BridgeError::Io(format!("ViGEm plugin: {e}")))?;
        target.wait_ready().map_err(|e| BridgeError::Io(format!("ViGEm wait_ready: {e}")))?;
        let notify = target
            .request_notification()
            .map(|n| n.spawn_thread(move |_, x| on_rumble(x.large_motor, x.small_motor)))
            .ok();
        let mut pad = VirtualPad { target: Some(target), notify, last: None };
        // A fresh target ignores an all-zero first report and XInput shows stale stick values until the
        // state changes (measured: L=(-3356,-1869)). The Stadia pad only reports on change, so a game could
        // see that drift until the stick moves. Prime with a 1-unit nudge, then centre.
        pad.submit(&XOut { lx: 1, ..XOut::default() });
        pad.submit(&XOut::default());
        Ok(pad)
    }

    /// Sends the state unless it is identical to the last one (games poll XInput; unchanged frames are free).
    pub fn submit(&mut self, o: &XOut) -> bool {
        if self.last.as_ref() == Some(o) {
            return true;
        }
        let Some(t) = self.target.as_mut() else { return false };
        let g = XGamepad {
            buttons: XButtons { raw: o.buttons },
            left_trigger: o.lt,
            right_trigger: o.rt,
            thumb_lx: o.lx,
            thumb_ly: o.ly,
            thumb_rx: o.rx,
            thumb_ry: o.ry,
        };
        let ok = t.update(&g).is_ok();
        if ok {
            self.last = Some(*o);
        }
        ok
    }
}

impl Drop for VirtualPad {
    fn drop(&mut self) {
        // Unplugging aborts the pending notification request, which ends the notification thread.
        drop(self.target.take());
        if let Some(h) = self.notify.take() {
            let _ = h.join();
        }
    }
}
