//! Push-to-talk voice commands. Uses the speech recognizer built into Windows,
//! which runs on this PC. Press the voice shortcut, say a command, and the
//! result comes back as a notification.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Media::SpeechRecognition::{SpeechRecognitionResultStatus, SpeechRecognizer};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

use crate::{actions, notify};

static LISTENING: AtomicBool = AtomicBool::new(false);

/// Start one listening session on a background thread. Ignored while one is running.
pub fn listen() {
    if LISTENING.swap(true, Ordering::SeqCst) {
        return;
    }
    notify::toast("WNTR", "Listening…");
    std::thread::spawn(|| {
        let reply = match recognize() {
            Ok(text) if text.trim().is_empty() => "Didn't catch that".to_string(),
            Ok(text) => format!("\"{}\": {}", text.trim(), actions::run(&text)),
            Err(e) => format!("Voice failed: {e}"),
        };
        LISTENING.store(false, Ordering::SeqCst);
        notify::toast("WNTR", &reply);
    });
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
