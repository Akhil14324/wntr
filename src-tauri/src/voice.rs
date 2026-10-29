//! Push-to-talk voice commands. Uses the speech recognizer built into Windows,
//! which runs on this PC. Press the voice shortcut, say a command, and each step
//! is reported to the notch ("voice" event) and, for the final result, a toast.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use windows::Foundation::TimeSpan;
use windows::Media::SpeechRecognition::{SpeechRecognitionResultStatus, SpeechRecognizer};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

use crate::{actions, notify};

/// Windows timespans are counted in 100-nanosecond ticks.
const TICKS_PER_SECOND: i64 = 10_000_000;

static LISTENING: AtomicBool = AtomicBool::new(false);

/// Start one listening session on a background thread. Ignored while one is running.
pub fn listen() {
    if LISTENING.swap(true, Ordering::SeqCst) {
        notify::emit("voice", json!({ "state": "error", "message": "Already listening" }));
        return;
    }
    notify::emit("voice", json!({ "state": "listening" }));
    std::thread::spawn(|| {
        match recognize() {
            Ok((text, status)) if text.trim().is_empty() => fail(&format!("Didn't catch that ({status:?})")),
            Ok((text, _)) => {
                let said = text.trim().to_string();
                notify::emit("voice", json!({ "state": "heard", "text": said }));
                let reply = actions::run(&said);
                LISTENING.store(false, Ordering::SeqCst);
                notify::emit("voice", json!({ "state": "done", "text": said, "reply": reply }));
                notify::toast("WNTR", &format!("\"{said}\": {reply}"));
                return;
            }
            Err(e) => fail(&format!("Voice failed: {e}")),
        }
        LISTENING.store(false, Ordering::SeqCst);
    });
}

fn fail(message: &str) {
    notify::emit("voice", json!({ "state": "error", "message": message }));
    notify::toast("WNTR", message);
}

/// One utterance using the default dictation grammar. Blocks until speech ends.
/// The recognizer is always closed afterwards: an open one keeps Windows' speech bar on screen.
fn recognize() -> windows::core::Result<(String, SpeechRecognitionResultStatus)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let recognizer = SpeechRecognizer::new()?;
    let outcome = recognize_with(&recognizer);
    let _ = recognizer.Close();
    outcome
}

fn recognize_with(recognizer: &SpeechRecognizer) -> windows::core::Result<(String, SpeechRecognitionResultStatus)> {
    // No "did you mean" confirmation, and stop listening after a short silence so
    // the session can't hang around.
    recognizer.UIOptions()?.SetShowConfirmation(false)?;
    let timeouts = recognizer.Timeouts()?;
    timeouts.SetInitialSilenceTimeout(TimeSpan { Duration: 8 * TICKS_PER_SECOND })?;
    timeouts.SetEndSilenceTimeout(TimeSpan { Duration: TICKS_PER_SECOND })?;
    recognizer.CompileConstraintsAsync()?.join()?;
    let result = recognizer.RecognizeAsync()?.join()?;
    let status = result.Status()?;
    if status != SpeechRecognitionResultStatus::Success {
        return Ok((String::new(), status));
    }
    Ok((result.Text()?.to_string(), status))
}
