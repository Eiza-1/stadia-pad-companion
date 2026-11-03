//! Live audio: the mic (any device format, converted to 16 kHz mono) and Gemini's voice (24 kHz,
//! resampled to the speakers), through WASAPI via cpal. Playback can be cleared at once (interrupt,
//! End). Everything stops when the `Mic` / `Player` is dropped.

use crate::live::{level, resample_24k_to, to_mono_16k};
use crate::log;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Samples waiting to play, at the output device's rate (mono; the stream copies to every channel).
pub struct AudioQueue {
    q: VecDeque<f32>,
    rate: u32,
}

impl AudioQueue {
    pub fn new(rate: u32) -> AudioQueue {
        AudioQueue { q: VecDeque::new(), rate: rate.max(1) }
    }
    pub fn push(&mut self, samples: &[f32]) {
        self.q.extend(samples);
    }
    /// The next `n` samples (silence once empty).
    pub fn pull(&mut self, n: usize) -> Vec<f32> {
        (0..n).map(|_| self.q.pop_front().unwrap_or(0.0)).collect()
    }
    pub fn clear(&mut self) {
        self.q.clear();
    }
    pub fn queued_ms(&self) -> u64 {
        self.q.len() as u64 * 1000 / self.rate as u64
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const MIC_CHUNK: usize = 1600; // 100 ms at 16 kHz

/// The default mic, delivered as ~100 ms chunks of 16 kHz mono. Capture stops when dropped.
pub struct Mic {
    stop: mpsc::Sender<()>,
}

impl Mic {
    pub fn start(on_chunk: impl FnMut(Vec<i16>) + Send + 'static) -> Result<Mic, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready_tx, ready) = mpsc::channel::<Result<(), String>>();
        std::thread::Builder::new()
            .name("live-mic".into())
            .spawn(move || {
                // cpal streams stay on the thread that made them.
                match build_mic(on_chunk) {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        let _ = stopped.recv(); // until the Mic is dropped
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        ready.recv().map_err(|_| "The mic stopped".to_string())??;
        Ok(Mic { stop })
    }
}

fn build_mic(mut on_chunk: impl FnMut(Vec<i16>) + Send + 'static) -> Result<cpal::Stream, String> {
    let dev = cpal::default_host().default_input_device().ok_or("No microphone found")?;
    let cfg = dev.default_input_config().map_err(|e| mic_error(&e.to_string()))?;
    let (ch, rate) = (cfg.channels(), cfg.sample_rate().0);
    let mut buf: Vec<i16> = Vec::with_capacity(MIC_CHUNK * 2);
    let mut feed = move |f: &[f32]| {
        buf.extend(to_mono_16k(f, ch, rate));
        while buf.len() >= MIC_CHUNK {
            on_chunk(buf.drain(..MIC_CHUNK).collect());
        }
    };
    let err = |e: cpal::StreamError| log!("mic stream: {e}");
    let format = cfg.sample_format();
    let config: cpal::StreamConfig = cfg.into();
    let s = match format {
        cpal::SampleFormat::F32 => dev.build_input_stream(&config, move |d: &[f32], _: &_| feed(d), err, None),
        cpal::SampleFormat::I16 => dev.build_input_stream(
            &config,
            move |d: &[i16], _: &_| feed(&d.iter().map(|&x| x as f32 / i16::MAX as f32).collect::<Vec<_>>()),
            err,
            None,
        ),
        f => return Err(format!("Unsupported mic format {f:?}")),
    }
    .map_err(|e| mic_error(&e.to_string()))?;
    s.play().map_err(|e| mic_error(&e.to_string()))?;
    Ok(s)
}

impl Drop for Mic {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

fn mic_error(e: &str) -> String {
    log!("mic: {e}");
    if e.contains("denied") || e.contains("0x80070005") {
        "Windows is blocking the mic. Turn on Settings > Privacy > Microphone for desktop apps".into()
    } else {
        "Couldn't open the microphone".into()
    }
}

type Ready = Result<(u32, Arc<Mutex<AudioQueue>>), String>;

/// Gemini's voice on the default speakers. `push` 24 kHz audio; `clear` silences it at once.
pub struct Player {
    queue: Arc<Mutex<AudioQueue>>,
    level: Arc<AtomicU32>,
    rate: u32,
    stop: mpsc::Sender<()>,
}

impl Player {
    pub fn start() -> Result<Player, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready_tx, ready) = mpsc::channel::<Ready>();
        let level = Arc::new(AtomicU32::new(0));
        let lv = level.clone();
        std::thread::Builder::new()
            .name("live-player".into())
            .spawn(move || match build_player(lv) {
                Ok((stream, rate, queue)) => {
                    let _ = ready_tx.send(Ok((rate, queue)));
                    let _ = stopped.recv();
                    drop(stream);
                }
                Err(e) => {
                    log!("speakers: {e}");
                    let _ = ready_tx.send(Err("Couldn't play sound on the speakers".into()));
                }
            })
            .map_err(|e| e.to_string())?;
        let (rate, queue) = ready.recv().map_err(|_| "The speakers stopped".to_string())??;
        Ok(Player { queue, level, rate, stop })
    }

    /// Queue Gemini's 24 kHz audio.
    pub fn push(&self, pcm24k: &[i16]) {
        let s = resample_24k_to(pcm24k, self.rate);
        lock(&self.queue).push(&s);
    }

    /// Drop everything still queued: silence at once.
    pub fn clear(&self) {
        lock(&self.queue).clear();
    }

    pub fn queued_ms(&self) -> u64 {
        lock(&self.queue).queued_ms()
    }

    /// Loudness of what is playing now (0..1).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

fn build_player(lv: Arc<AtomicU32>) -> Result<(cpal::Stream, u32, Arc<Mutex<AudioQueue>>), String> {
    let dev = cpal::default_host().default_output_device().ok_or("No speakers found")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let (ch, rate) = (cfg.channels() as usize, cfg.sample_rate().0);
    let queue = Arc::new(Mutex::new(AudioQueue::new(rate)));
    let q = queue.clone();
    let fill = move |n_frames: usize| -> Vec<f32> {
        let mono = lock(&q).pull(n_frames);
        let pcm: Vec<i16> = mono.iter().map(|x| (x * i16::MAX as f32) as i16).collect();
        lv.store(level(&pcm).to_bits(), Ordering::Relaxed);
        mono
    };
    let err = |e: cpal::StreamError| log!("speaker stream: {e}");
    let format = cfg.sample_format();
    let config: cpal::StreamConfig = cfg.into();
    let s = match format {
        cpal::SampleFormat::F32 => dev.build_output_stream(
            &config,
            move |d: &mut [f32], _: &_| {
                let m = fill(d.len() / ch);
                for (f, x) in d.chunks_mut(ch).zip(m) {
                    f.fill(x);
                }
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => dev.build_output_stream(
            &config,
            move |d: &mut [i16], _: &_| {
                let m = fill(d.len() / ch);
                for (f, x) in d.chunks_mut(ch).zip(m) {
                    f.fill((x * i16::MAX as f32) as i16);
                }
            },
            err,
            None,
        ),
        f => return Err(format!("Unsupported speaker format {f:?}")),
    }
    .map_err(|e| e.to_string())?;
    s.play().map_err(|e| e.to_string())?;
    Ok((s, rate, queue))
}

impl Drop for Player {
    fn drop(&mut self) {
        self.clear();
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_clear_drops_queued_audio() {
        let mut q = AudioQueue::new(24_000);
        q.push(&vec![0.5; 24_000]);
        assert_eq!(q.queued_ms(), 1000);
        assert_eq!(q.pull(3), vec![0.5; 3]);
        q.clear();
        assert_eq!(q.queued_ms(), 0);
        assert_eq!(q.pull(4), vec![0.0; 4], "silence after clear");
    }
}
