//! Notch window placement, cursor hit-testing (click-through when collapsed),
//! fullscreen hiding and idle detection.

use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_LBUTTON};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::{notify, settings, util::sleep_ms};

/// Logical size of the transparent notch window. Only the hit rects reported by
/// the UI receive mouse input; everything else is click-through.
pub const WIN_W: f64 = 820.0;
pub const WIN_H: f64 = 520.0;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Default)]
struct Geom {
    x: i32,
    y: i32,
    scale: f64,
}

static HIT: Mutex<Vec<Rect>> = Mutex::new(Vec::new());
static GEOM: Mutex<Geom> = Mutex::new(Geom { x: 0, y: 0, scale: 1.0 });
static HIDDEN_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static MONITOR_W: Mutex<f64> = Mutex::new(1920.0);

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

/// Move the notch to the top-center of the configured monitor.
pub fn place(app: &AppHandle) {
    let Some(win) = window(app) else { return };
    let wanted = settings::get_str("general.monitor");
    let monitors = win.available_monitors().unwrap_or_default();
    let monitor = monitors
        .iter()
        .find(|m| wanted != "primary" && m.name().map(|n| n == &wanted).unwrap_or(false))
        .cloned()
        .or_else(|| win.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let w = (WIN_W * scale).round() as u32;
    let h = (WIN_H * scale).round() as u32;
    let x = m.position().x + (m.size().width as i32 - w as i32) / 2;
    let y = m.position().y;
    let _ = win.set_size(PhysicalSize::new(w, h));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    *GEOM.lock() = Geom { x, y, scale };
    *MONITOR_W.lock() = m.size().width as f64 / scale;
    notify::emit(
        "notch-monitor",
        json!({ "width": m.size().width as f64 / scale, "scale": scale }),
    );
}

pub fn on_moved(win: &WebviewWindow) {
    if let (Ok(pos), Ok(scale)) = (win.outer_position(), win.scale_factor()) {
        *GEOM.lock() = Geom {
            x: pos.x,
            y: pos.y,
            scale,
        };
    }
}

fn button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

/// Polls the cursor and toggles click-through so only the visible notch takes input.
pub fn start(app: AppHandle) {
    if let Some(win) = window(&app) {
        let _ = win.set_ignore_cursor_events(true);
    }
    let hover_app = app.clone();
    crate::util::spawn_loop("notch-hover", move || {
        let mut last_inside = false;
        loop {
            let mut pt = POINT::default();
            let ok = unsafe { GetCursorPos(&mut pt).is_ok() };
            let g = *GEOM.lock();
            let lx = (pt.x - g.x) as f64 / g.scale;
            let ly = (pt.y - g.y) as f64 / g.scale;
            let in_window = ok && lx >= 0.0 && ly >= 0.0 && lx < WIN_W && ly < WIN_H;
            let inside = in_window && HIT.lock().iter().any(|r| r.contains(lx, ly));
            if inside != last_inside {
                let pressing = button_down();
                // Keep input while a press that started inside is being dragged out
                // (sliders, text selection). Native drag-out manages itself.
                let hold = !inside && pressing && !crate::drag::drag_out_active();
                if !hold {
                    last_inside = inside;
                    if let Some(win) = window(&hover_app) {
                        let _ = win.set_ignore_cursor_events(!inside);
                    }
                    notify::emit("notch-hover", json!({ "inside": inside, "pressing": pressing }));
                }
            }
            sleep_ms(if in_window { 30 } else { 90 });
        }
    });

    crate::util::spawn_loop("notch-watch", move || {
        let mut tick: u64 = 0;
        let mut last_idle_bucket = u64::MAX;
        loop {
            // Fullscreen apps (games, videos, presentations) hide the notch.
            let busy = unsafe {
                matches!(
                    SHQueryUserNotificationState(),
                    Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE
                )
            };
            let hide = busy && settings::get_bool("general.hideInFullscreen", true);
            if hide != HIDDEN_FULLSCREEN.load(Ordering::Relaxed) {
                HIDDEN_FULLSCREEN.store(hide, Ordering::Relaxed);
                if let Some(win) = window(&app) {
                    let _ = if hide { win.hide() } else { win.show() };
                }
            }
            // Re-assert topmost now and then; other topmost windows can cover us.
            if tick.is_multiple_of(5)
                && !hide
                && let Some(win) = window(&app)
            {
                let _ = win.set_always_on_top(true);
            }
            let idle = idle_secs();
            let bucket = idle / 5;
            if bucket != last_idle_bucket {
                last_idle_bucket = bucket;
                notify::emit("idle", json!({ "secs": idle }));
            }
            tick += 1;
            sleep_ms(2000);
        }
    });
}

pub fn idle_secs() -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut info).as_bool() {
            let now = windows::Win32::System::SystemInformation::GetTickCount();
            return (now.wrapping_sub(info.dwTime) / 1000) as u64;
        }
    }
    0
}

#[tauri::command]
pub fn notch_set_hit(rects: Vec<Rect>) {
    *HIT.lock() = rects;
}

#[tauri::command]
pub fn notch_focus(app: AppHandle) {
    if let Some(win) = window(&app) {
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.set_focus();
    }
}

#[tauri::command]
pub fn notch_info(app: AppHandle) -> serde_json::Value {
    let monitors: Vec<String> = window(&app)
        .and_then(|w| w.available_monitors().ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m.name().cloned())
        .collect();
    json!({ "monitorWidth": *MONITOR_W.lock(), "monitors": monitors, "idleSecs": idle_secs() })
}
