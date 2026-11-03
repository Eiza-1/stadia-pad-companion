//! Registers the overlay shortcut with Windows (RegisterHotKey: no keyboard hook, Windows tells us only
//! about this combination) and tells tap from hold by checking the key only while it is held.

use crate::hotkey::{validate, Hotkey, KeyHold};
use crate::special::Action;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, PM_REMOVE, WM_APP, WM_HOTKEY, WM_QUIT, WM_USER,
};

const WM_SET: u32 = WM_APP + 1;
const ID: i32 = 1;
const POLL: Duration = Duration::from_millis(15);

type Request = (Option<Hotkey>, Sender<Result<(), String>>);

pub struct HotkeyThread {
    tid: u32,
    pending: Arc<Mutex<Option<Request>>>,
}

fn mods(h: &Hotkey) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    for (on, bit) in [(h.ctrl, MOD_CONTROL), (h.alt, MOD_ALT), (h.shift, MOD_SHIFT), (h.win, MOD_WIN)] {
        if on {
            m |= bit;
        }
    }
    m
}

impl HotkeyThread {
    /// Starts the thread; `on` gets OverlayTap / TalkStart / TalkEnd. Register a shortcut with `set`.
    pub fn start(on: impl Fn(Action) + Send + 'static) -> HotkeyThread {
        let pending: Arc<Mutex<Option<Request>>> = Arc::new(Mutex::new(None));
        let (tid_tx, tid_rx) = mpsc::channel();
        let p = pending.clone();
        let _ = std::thread::Builder::new().name("overlay-hotkey".into()).spawn(move || {
            // Create this thread's message queue before anyone can post to it.
            let mut warm = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut warm, None, WM_USER, WM_USER, PM_NOREMOVE);
            }
            let _ = tid_tx.send(unsafe { GetCurrentThreadId() });
            let t0 = Instant::now();
            let ms = || t0.elapsed().as_millis() as u64;
            let mut current: Option<Hotkey> = None;
            let mut msg = MSG::default();
            while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                match msg.message {
                    WM_SET => {
                        let Some((want, reply)) = p.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
                        unsafe {
                            let _ = UnregisterHotKey(None, ID);
                        }
                        let r = match want {
                            None => Ok(()),
                            Some(h) => unsafe { RegisterHotKey(None, ID, mods(&h), h.vk as u32) }
                                .map_err(|_| "That shortcut is already used by Windows or another app".to_string()),
                        };
                        if r.is_ok() {
                            current = want;
                        } else if let Some(old) = current {
                            // Keep the shortcut that worked.
                            unsafe {
                                let _ = RegisterHotKey(None, ID, mods(&old), old.vk as u32);
                            }
                        }
                        let _ = reply.send(r);
                    }
                    WM_HOTKEY => {
                        let Some(h) = current else { continue };
                        let mut hold = KeyHold::default();
                        hold.pressed(ms());
                        // Only now, and only until release plus the double-tap wait, is the key looked at.
                        while hold.active() {
                            std::thread::sleep(POLL);
                            let down = unsafe { GetAsyncKeyState(h.vk as i32) } < 0;
                            if let Some(a) = hold.poll(ms(), down) {
                                on(a);
                            }
                        }
                        // A second press during the wait was already seen above; drop its own WM_HOTKEY
                        // so it doesn't start a new tap.
                        let mut stale = MSG::default();
                        while unsafe { PeekMessageW(&mut stale, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) }.as_bool() {}
                    }
                    _ => {}
                }
            }
            unsafe {
                let _ = UnregisterHotKey(None, ID);
            }
        });
        let tid = tid_rx.recv().unwrap_or(0);
        HotkeyThread { tid, pending }
    }

    /// Registers `h` (or none). On failure the previous shortcut stays registered.
    pub fn set(&self, h: Option<Hotkey>) -> Result<(), String> {
        if let Some(k) = &h {
            validate(k).map_err(str::to_string)?;
        }
        let (tx, rx) = mpsc::channel();
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((h, tx));
        unsafe { PostThreadMessageW(self.tid, WM_SET, Default::default(), Default::default()) }
            .map_err(|e| e.message().to_string())?;
        rx.recv_timeout(Duration::from_secs(2)).map_err(|_| "Couldn't set the shortcut".to_string())?
    }
}

impl Drop for HotkeyThread {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.tid, WM_QUIT, Default::default(), Default::default());
        }
    }
}
