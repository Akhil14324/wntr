use parking_lot::RwLock;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::util::{CmdResult, data_dir, err};

static SETTINGS: once_cell::sync::Lazy<RwLock<Value>> = once_cell::sync::Lazy::new(|| RwLock::new(defaults()));

pub fn defaults() -> Value {
    json!({
        "general": {
            "autostart": true,
            "monitor": "primary",
            "notchSize": "medium",
            "customWidth": 210,
            "customHeight": 34,
            "autoLarger": true,
            "minimalMode": false,
            "hotkey": "Ctrl+Alt+N",
            "hideInFullscreen": true,
            "expandDelayMs": 120,
            "collapseDelayMs": 450,
            "pages": ["now", "music", "shelf", "focus", "water", "todo",
                      "notes", "clipboard", "launcher", "weather", "timers", "stats"]
        },
        "appearance": {
            "theme": "dark",
            "accent": "#7dd3fc",
            "notchColor": "#000000",
            "notchOpacity": 1.0,
            "mascot": true,
            "mascotIdleMin": 5
        },
        "music": {
            "lyricOffsets": {},
            "lyricsInPill": true,
            "sources": { "chrome": true, "edge": true, "firefox": true, "spotify": true, "other": true },
            "showArt": true
        },
        "focus": {
            "focusMin": 25, "shortMin": 5, "longMin": 15, "cycles": 4,
            "chime": true, "autoStartBreaks": true
        },
        "water": {
            "bottles": [250, 500],
            "goal": 1500,
            "intervalMin": 60,
            "start": "09:00",
            "end": "22:00",
            "reminders": true
        },
        "overview": {
            "widgets": {
                "now": { "w": 4, "h": 2 },
                "task": { "w": 3, "h": 1 },
                "event": { "w": 3, "h": 1 },
                "focus": { "w": 3, "h": 1 },
                "water": { "w": 3, "h": 1 }
            }
        },
        "tasks": { "defaultReminderMin": 0, "snooze": [5, 10, 30] },
        "calendar": { "alertMin": 10, "refreshMin": 15 },
        "status": {
            "battery": true, "wifi": true, "bluetooth": true, "capsLock": true,
            "keyboardLang": true, "volume": true, "brightness": true,
            "lowBattery": [20, 10]
        },
        "clipboard": { "size": 50, "paused": false },
        "notes": { "exportDir": "" },
        "weather": { "city": "", "lat": null, "lon": null, "place": "", "unit": "celsius" },
        "dnd": { "until": 0, "presets": [30, 60] }
    })
}

/// Recursively merge `patch` into `base`. Objects merge; everything else replaces.
pub fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn file() -> std::path::PathBuf {
    data_dir().join("settings.json")
}

pub fn load() {
    let mut s = defaults();
    if let Ok(text) = std::fs::read_to_string(file())
        && let Ok(saved) = serde_json::from_str::<Value>(&text)
    {
        merge(&mut s, &saved);
        // Pages list is replaced wholesale; make sure new pages show up after upgrades.
        let all = defaults()["general"]["pages"].clone();
        if let (Some(cur), Some(all)) = (s["general"]["pages"].as_array().cloned(), all.as_array())
            && cur.is_empty()
        {
            s["general"]["pages"] = Value::Array(all.clone());
        }
    }
    *SETTINGS.write() = s;
}

fn save(v: &Value) {
    if let Ok(text) = serde_json::to_string_pretty(v) {
        let tmp = file().with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, file());
        }
    }
}

pub fn all() -> Value {
    SETTINGS.read().clone()
}

/// Read a value by dotted path, e.g. `get("water.goal")`.
pub fn get(path: &str) -> Value {
    let s = SETTINGS.read();
    let mut cur = &*s;
    for part in path.split('.') {
        match cur.get(part) {
            Some(v) => cur = v,
            None => return Value::Null,
        }
    }
    cur.clone()
}

pub fn get_i64(path: &str, default: i64) -> i64 {
    get(path)
        .as_i64()
        .or_else(|| get(path).as_f64().map(|f| f as i64))
        .unwrap_or(default)
}

pub fn get_bool(path: &str, default: bool) -> bool {
    get(path).as_bool().unwrap_or(default)
}

pub fn get_str(path: &str) -> String {
    get(path).as_str().unwrap_or_default().to_string()
}

pub fn patch(app: &AppHandle, patch: &Value) -> Value {
    let (old, new) = {
        let mut s = SETTINGS.write();
        let old = s.clone();
        merge(&mut s, patch);
        (old, s.clone())
    };
    save(&new);
    let _ = app.emit("settings", &new);
    crate::apply_settings(app, &old, &new);
    new
}

pub fn replace_all(app: &AppHandle, v: Value) {
    let mut s = defaults();
    merge(&mut s, &v);
    let old = std::mem::replace(&mut *SETTINGS.write(), s.clone());
    save(&s);
    let _ = app.emit("settings", &s);
    crate::apply_settings(app, &old, &s);
}

#[tauri::command]
pub fn settings_get() -> Value {
    all()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, patch: Value) -> CmdResult<Value> {
    if !patch.is_object() {
        return Err(err("settings patch must be an object"));
    }
    Ok(self::patch(&app, &patch))
}

#[tauri::command]
pub fn settings_reset(app: AppHandle) -> Value {
    replace_all(&app, defaults());
    all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_is_deep_for_objects_and_replaces_arrays() {
        let mut base = json!({"a": {"b": 1, "c": [1, 2]}, "d": 1});
        merge(&mut base, &json!({"a": {"c": [3]}, "e": true}));
        assert_eq!(base, json!({"a": {"b": 1, "c": [3]}, "d": 1, "e": true}));
    }
}
