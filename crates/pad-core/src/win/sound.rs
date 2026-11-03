//! Plays a WAV file from memory (Gemini's read-aloud voice) without blocking, and stops it.
//! Windows keeps reading the buffer while it plays, so the bytes live here until the next play or stop.

use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

static PLAYING: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Starts playing `wav` (stopping anything already playing). Returns false if Windows refused it.
pub fn play(wav: Vec<u8>) -> bool {
    let mut slot = PLAYING.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), None, SND_NODEFAULT); // stop first, before the old buffer goes
    }
    *slot = Some(wav);
    let ptr = slot.as_ref().map(|b| b.as_ptr()).unwrap_or(std::ptr::null());
    unsafe { PlaySoundW(PCWSTR(ptr as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT).as_bool() }
}

/// Stops the voice, if one is playing.
pub fn stop() {
    let mut slot = PLAYING.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), None, SND_NODEFAULT);
    }
    *slot = None;
}
