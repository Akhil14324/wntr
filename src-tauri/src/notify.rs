//! App handle access plus the gated channels for popups, alerts and toasts.

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::{settings, util::now_ms};

static APP: once_cell::sync::OnceCell<AppHandle> = once_cell::sync::OnceCell::new();

pub fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

pub fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

pub fn dnd_active() -> bool {
    settings::get_i64("dnd.until", 0) > now_ms()
}

/// Windows toast. Suppressed while Do Not Disturb is on.
pub fn toast(title: &str, body: &str) {
    if dnd_active() {
        return;
    }
    if let Some(app) = APP.get() {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

/// Short-lived status popup in the notch (battery, Wi-Fi, ...). `setting` is the
/// `status.*` toggle that controls it.
pub fn popup(setting: &str, kind: &str, data: Value) {
    if dnd_active() || !settings::get_bool(&format!("status.{setting}"), true) {
        return;
    }
    emit("popup", json!({ "kind": kind, "data": data, "at": now_ms() }));
}

/// Actionable alert shown in the notch plus a toast (reminders, timers, events).
pub fn alert(kind: &str, title: &str, body: &str, data: Value) {
    if dnd_active() {
        return;
    }
    emit(
        "alert",
        json!({ "kind": kind, "title": title, "body": body, "data": data, "at": now_ms() }),
    );
    toast(title, body);
}

/// Unpackaged (portable) builds have no Start-menu shortcut, so Windows drops their
/// toasts unless the AppUserModelID is registered. This does that per-user.
pub fn register_aumid(identifier: &str) {
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;
    let key_path = crate::util::wide(&format!("Software\\Classes\\AppUserModelId\\{identifier}"));
    unsafe {
        let mut hkey = HKEY::default();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_ok()
        {
            let name = crate::util::wide("WNTR");
            let bytes = std::slice::from_raw_parts(name.as_ptr() as *const u8, name.len() * 2);
            let value_name = crate::util::wide("DisplayName");
            let _ = RegSetValueExW(hkey, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes));
            let _ = RegCloseKey(hkey);
        }
        let id = crate::util::wide(identifier);
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(PCWSTR(id.as_ptr()));
    }
}
