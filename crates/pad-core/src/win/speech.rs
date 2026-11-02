//! Live dictation for the Gemini overlay with Windows' own speech recognition: partial text while you
//! speak, the full transcript when you let go. Runs on its own thread only while listening.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use windows::core::{Interface, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionSession, SpeechContinuousRecognitionResultGeneratedEventArgs,
    SpeechRecognitionCompilationResult, SpeechRecognitionHypothesisGeneratedEventArgs, SpeechRecognitionResultStatus,
    SpeechRecognitionScenario, SpeechRecognitionTopicConstraint, SpeechRecognizer,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SpeechEvent {
    /// Everything heard so far, including the words still being recognised.
    Partial(String),
    /// Everything heard so far, settled.
    Final(String),
    Error(&'static str),
}

const PRIVACY: &str = "Turn on Online speech recognition in Windows Settings › Privacy › Speech";
const NO_SPEECH: &str = "No microphone or speech language found";
/// SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED: dictation needs Windows' online speech recognition setting.
const HR_PRIVACY: i32 = 0x8004_5509_u32 as i32;

fn error_for(e: &windows::core::Error) -> &'static str {
    if e.code().0 == HR_PRIVACY { PRIVACY } else { NO_SPEECH }
}

#[derive(Default)]
struct Heard {
    finals: String,
    partial: String,
}

impl Heard {
    fn all(&self) -> String {
        [self.finals.trim(), self.partial.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")
    }
}

pub struct Dictation {
    stop: Sender<Sender<String>>,
}

impl Dictation {
    /// Starts listening; `on` gets partial / final text or one error. Never blocks the caller.
    pub fn start(on: impl Fn(SpeechEvent) + Send + Sync + 'static) -> Dictation {
        let (stop_tx, stop_rx) = mpsc::channel::<Sender<String>>();
        let on = Arc::new(on);
        let _ = thread::Builder::new().name("pad-speech".into()).spawn(move || {
            crate::win::com_mta();
            let heard = Arc::new(Mutex::new(Heard::default()));
            let session = match listen(&heard, on.clone()) {
                Ok(s) => Some(s),
                Err(e) => {
                    on(SpeechEvent::Error(error_for(&e)));
                    None
                }
            };
            // Wait for stop(), then let the last words settle before answering.
            if let Ok(reply) = stop_rx.recv() {
                if let Some((s, _rec)) = &session {
                    if let Ok(op) = s.StopAsync() {
                        let _ = op.join();
                    }
                }
                let text = heard.lock().map(|h| h.all()).unwrap_or_default();
                let _ = reply.send(text);
            }
        });
        Dictation { stop: stop_tx }
    }

    /// Stops listening and returns everything heard (waits at most 1.5 s for the last words).
    pub fn stop(self) -> String {
        let (tx, rx) = mpsc::channel();
        if self.stop.send(tx).is_err() {
            return String::new();
        }
        rx.recv_timeout(Duration::from_millis(1500)).unwrap_or_default()
    }
}

fn listen(
    heard: &Arc<Mutex<Heard>>,
    on: Arc<dyn Fn(SpeechEvent) + Send + Sync>,
) -> windows::core::Result<(SpeechContinuousRecognitionSession, SpeechRecognizer)> {
    let rec = SpeechRecognizer::new()?;
    let topic = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &HSTRING::from("dictation"))?;
    rec.Constraints()?.Append(&topic.cast::<windows::Media::SpeechRecognition::ISpeechRecognitionConstraint>()?)?;
    let compiled: SpeechRecognitionCompilationResult = rec.CompileConstraintsAsync()?.join()?;
    if compiled.Status()? != SpeechRecognitionResultStatus::Success {
        return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
    }
    {
        let (heard, on) = (heard.clone(), on.clone());
        rec.HypothesisGenerated(&TypedEventHandler::<SpeechRecognizer, SpeechRecognitionHypothesisGeneratedEventArgs>::new(
            move |_, args| {
                if let Some(a) = args.as_ref() {
                    let text = a.Hypothesis()?.Text()?.to_string();
                    let all = {
                        let mut h = heard.lock().unwrap_or_else(|e| e.into_inner());
                        h.partial = text;
                        h.all()
                    };
                    on(SpeechEvent::Partial(all));
                }
                Ok(())
            },
        ))?;
    }
    let session = rec.ContinuousRecognitionSession()?;
    {
        let (heard, on) = (heard.clone(), on.clone());
        session.ResultGenerated(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionResultGeneratedEventArgs,
        >::new(move |_, args| {
            if let Some(a) = args.as_ref() {
                let text = a.Result()?.Text()?.to_string();
                let all = {
                    let mut h = heard.lock().unwrap_or_else(|e| e.into_inner());
                    if !text.trim().is_empty() {
                        if !h.finals.is_empty() {
                            h.finals.push(' ');
                        }
                        h.finals.push_str(text.trim());
                    }
                    h.partial.clear();
                    h.all()
                };
                on(SpeechEvent::Final(all));
            }
            Ok(())
        }))?;
    }
    session.StartAsync()?.join()?;
    Ok((session, rec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heard_joins_finals_and_partial() {
        let h = Heard { finals: "what's scorpion's".into(), partial: "spear combo".into() };
        assert_eq!(h.all(), "what's scorpion's spear combo");
        assert_eq!(Heard::default().all(), "");
    }

    #[test]
    fn privacy_error_has_its_own_message() {
        let e = windows::core::Error::from_hresult(windows::core::HRESULT(HR_PRIVACY));
        assert_eq!(error_for(&e), PRIVACY);
        let other = windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL);
        assert_eq!(error_for(&other), NO_SPEECH);
    }
}
