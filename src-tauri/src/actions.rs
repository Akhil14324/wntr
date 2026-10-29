//! Local command runner for voice (and typed) commands. Phrases map to fixed
//! actions: launching apps, media and volume keys, typing, and a few system
//! actions. Nothing here calls a network service or an AI model.
//!
//! Spoken text never reaches a shell. App names go through ShellExecute, and
//! anything else is typed as plain keystrokes.

use std::mem::size_of;
use std::process::Command;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
    VIRTUAL_KEY, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MENU,
    VK_RETURN, VK_S, VK_SHIFT, VK_TAB, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use crate::util::wide;

/// Friendly name → what ShellExecute should open.
const APPS: &[(&str, &str)] = &[
    ("notepad", "notepad.exe"),
    ("calculator", "calc.exe"),
    ("paint", "mspaint.exe"),
    ("chrome", "chrome.exe"),
    ("google chrome", "chrome.exe"),
    ("edge", "msedge.exe"),
    ("microsoft edge", "msedge.exe"),
    ("firefox", "firefox.exe"),
    ("file explorer", "explorer.exe"),
    ("explorer", "explorer.exe"),
    ("task manager", "taskmgr.exe"),
    ("settings", "ms-settings:"),
    ("terminal", "wt.exe"),
    ("command prompt", "cmd.exe"),
    ("downloads", "shell:Downloads"),
    ("documents", "shell:Personal"),
    ("pictures", "shell:My Pictures"),
    ("desktop", "shell:Desktop"),
];

/// Run one phrase and return a short reply for the user.
pub fn run(phrase: &str) -> String {
    let orig = phrase.trim().trim_end_matches(['.', '!', '?']).trim();
    let lower = orig.to_lowercase();

    for prefix in ["open ", "launch ", "start "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return open_app(rest.trim());
        }
    }
    if let Some(query) = lower.strip_prefix("search for ").or(lower.strip_prefix("search ")) {
        let url = format!("https://www.bing.com/search?q={}", percent_encode(query.trim()));
        return match shell_open(&url) {
            true => format!("Searching for {}", query.trim()),
            false => "Couldn't open the browser".into(),
        };
    }
    if let Some(text) = orig.get(5..).filter(|_| lower.starts_with("type ")) {
        type_text(text);
        return format!("Typed {} characters", text.chars().count());
    }

    match lower.as_str() {
        "lock" | "lock the computer" | "lock screen" => {
            let _ = Command::new("rundll32.exe").arg("user32.dll,LockWorkStation").spawn();
            "Locked".into()
        }
        "sleep" | "sleep the computer" => {
            let _ = Command::new("rundll32.exe").arg("powrprof.dll,SetSuspendState").arg("0,1,0").spawn();
            "Sleeping".into()
        }
        "screenshot" | "take a screenshot" | "take screenshot" => {
            tap(&[VK_LWIN, VK_SHIFT, VK_S]);
            "Select an area to capture".into()
        }
        "volume up" | "louder" => tap_key(VK_VOLUME_UP, "Volume up"),
        "volume down" | "quieter" | "softer" => tap_key(VK_VOLUME_DOWN, "Volume down"),
        "mute" | "unmute" | "mute volume" => tap_key(VK_VOLUME_MUTE, "Toggled mute"),
        "play" | "pause" | "play pause" | "play or pause" => tap_key(VK_MEDIA_PLAY_PAUSE, "Play / pause"),
        "next" | "next track" | "skip" => tap_key(VK_MEDIA_NEXT_TRACK, "Next track"),
        "previous" | "previous track" | "back" => tap_key(VK_MEDIA_PREV_TRACK, "Previous track"),
        "switch window" | "switch app" | "alt tab" => {
            tap(&[VK_MENU, VK_TAB]);
            "Switched window".into()
        }
        "press enter" | "enter" => tap_key(VK_RETURN, "Pressed Enter"),
        _ => format!("I don't know how to \"{orig}\" yet"),
    }
}

fn open_app(name: &str) -> String {
    let target = APPS
        .iter()
        .find(|(friendly, _)| *friendly == name)
        .map(|(_, t)| *t)
        .unwrap_or(name);
    if target.is_empty() {
        return "Open what?".into();
    }
    if shell_open(target) {
        format!("Opened {name}")
    } else {
        format!("Couldn't find {name}")
    }
}

/// ShellExecute handles app names (App Paths), URIs and shell folders. It never
/// goes through a command shell, so the text is not interpreted.
fn shell_open(target: &str) -> bool {
    let file = wide(target);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a value above 32 on success.
    result.0 as isize > 32
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, size_of::<INPUT>() as i32);
    }
}

/// Press the keys in order, then release them in reverse order.
fn tap(keys: &[VIRTUAL_KEY]) {
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| key(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| key(*k, true)));
    send(&inputs);
}

fn tap_key(vk: VIRTUAL_KEY, reply: &str) -> String {
    tap(&[vk]);
    reply.into()
}

/// Type text as Unicode keystrokes, so it works in any app and keyboard layout.
fn type_text(text: &str) {
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        for up in [false, true] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    send(&inputs);
}

/// Encode a search query for a URL (unreserved characters pass through).
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_search_queries() {
        assert_eq!(percent_encode("rust lang & more"), "rust+lang+%26+more");
    }

    #[test]
    fn unknown_phrase_is_reported_not_run() {
        assert!(run("do the impossible").contains("don't know"));
    }
}
