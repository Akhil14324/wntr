//! Push-to-talk voice commands. Uses the speech recognizer built into Windows,
//! which runs on this PC. Press the voice shortcut, say a command, and each step
//! is reported to the notch ("voice" event) and, for the final result, a toast.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use windows::Media::SpeechRecognition::{SpeechRecognitionResultStatus, SpeechRecognizer};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

use crate::{actions, notify};

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
            Ok(text) if text.trim().is_empty() => fail("Didn't catch that"),
            Ok(text) => {
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
fn recognize() -> windows::core::Result<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let recognizer = SpeechRecognizer::new()?;
    recognizer.CompileConstraintsAsync()?.join()?;
    let result = recognizer.RecognizeAsync()?.join()?;
    if result.Status()? != SpeechRecognitionResultStatus::Success {
        return Ok(String::new());
    }
    Ok(result.Text()?.to_string())
}
