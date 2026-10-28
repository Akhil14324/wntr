//! Clipboard history. Watches the clipboard sequence number (no hooks, no key
//! reading) and stores text and images locally.

use std::sync::atomic::{AtomicU32, Ordering};

use rusqlite::params;
use serde::Serialize;
use windows::Win32::System::DataExchange::{
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, RegisterClipboardFormatW,
};
use windows::core::PCWSTR;

use crate::db::db;
use crate::util::{CmdResult, err, hash_bytes, now_ms, sleep_ms, sub_dir, wide};
use crate::{notify, settings};

static IGNORE_SEQ: AtomicU32 = AtomicU32::new(0);

/// Mark a clipboard change as ours so it is not recorded again.
pub fn ignore_seq(seq: u32) {
    IGNORE_SEQ.store(seq, Ordering::Relaxed);
}

const MAX_TEXT: usize = 200_000;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    id: i64,
    kind: String,
    text: Option<String>,
    image: Option<String>,
    pinned: bool,
    created_at: i64,
}

pub fn encode_png(img: &arboard::ImageData) -> Option<Vec<u8>> {
    let buf = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.to_vec())?;
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

pub fn set_text(text: &str) -> CmdResult<()> {
    let mut cb = arboard::Clipboard::new().map_err(err)?;
    cb.set_text(text.to_string()).map_err(err)?;
    ignore_seq(unsafe { GetClipboardSequenceNumber() });
    Ok(())
}

/// Password managers mark secrets with these formats; we never record them.
fn excluded_by_source() -> bool {
    for name in [
        "ExcludeClipboardContentFromMonitorProcessing",
        "Clipboard Viewer Ignore",
    ] {
        let w = wide(name);
        unsafe {
            let cf = RegisterClipboardFormatW(PCWSTR(w.as_ptr()));
            if cf != 0 && IsClipboardFormatAvailable(cf).is_ok() {
                return true;
            }
        }
    }
    false
}

fn record() {
    if excluded_by_source() {
        return;
    }
    let Ok(mut cb) = arboard::Clipboard::new() else { return };
    let now = now_ms();
    if let Ok(text) = cb.get_text() {
        if text.trim().is_empty() || text.len() > MAX_TEXT {
            return;
        }
        let hash = hash_bytes(text.as_bytes());
        upsert("text", Some(&text), None, &hash, now);
    } else if let Ok(img) = cb.get_image() {
        if img.width * img.height > 6000 * 6000 {
            return;
        }
        let hash = hash_bytes(&img.bytes);
        let exists: bool = db()
            .query_row("SELECT 1 FROM clipboard_items WHERE hash = ?1", [&hash], |_| Ok(()))
            .is_ok();
        if exists {
            upsert("image", None, None, &hash, now);
            return;
        }
        let Some(png) = encode_png(&img) else { return };
        let path = sub_dir("clipboard").join(format!("{hash}.png"));
        if std::fs::write(&path, png).is_ok() {
            upsert("image", None, Some(&path.to_string_lossy()), &hash, now);
        }
    } else {
        return;
    }
    trim();
    notify::emit("clipboard", serde_json::json!({}));
}

fn upsert(kind: &str, text: Option<&str>, image: Option<&str>, hash: &str, now: i64) {
    let conn = db();
    let updated = conn
        .execute(
            "UPDATE clipboard_items SET created_at = ?1 WHERE hash = ?2",
            params![now, hash],
        )
        .unwrap_or(0);
    if updated == 0 && (text.is_some() || image.is_some()) {
        let _ = conn.execute(
            "INSERT INTO clipboard_items(kind, text, image_path, hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![kind, text, image, hash, now],
        );
    }
}

/// Keep only the newest `clipboard.size` unpinned items.
fn trim() {
    let keep = settings::get_i64("clipboard.size", 50).clamp(20, 200);
    let conn = db();
    let doomed: Vec<(i64, Option<String>)> = conn
        .prepare(
            "SELECT id, image_path FROM clipboard_items WHERE pinned = 0 ORDER BY created_at DESC LIMIT -1 OFFSET ?1",
        )
        .and_then(|mut s| {
            s.query_map([keep], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(|rows| rows.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    for (id, image) in doomed {
        if let Some(p) = image {
            let _ = std::fs::remove_file(p);
        }
        let _ = conn.execute("DELETE FROM clipboard_items WHERE id = ?1", [id]);
    }
}

pub fn start() {
    crate::util::spawn_loop("clipboard-watch", || {
        let mut last = unsafe { GetClipboardSequenceNumber() };
        loop {
            sleep_ms(500);
            let seq = unsafe { GetClipboardSequenceNumber() };
            if seq == last {
                continue;
            }
            last = seq;
            if seq == IGNORE_SEQ.load(Ordering::Relaxed) || settings::get_bool("clipboard.paused", false) {
                continue;
            }
            // Give the source app a moment to finish writing all formats.
            sleep_ms(120);
            record();
        }
    });
}

#[tauri::command]
pub fn clipboard_list(query: Option<String>) -> Vec<ClipItem> {
    use base64::Engine;
    let q = query.unwrap_or_default().to_lowercase();
    let conn = db();
    let mut stmt = conn
        .prepare("SELECT id, kind, text, image_path, pinned, created_at FROM clipboard_items ORDER BY pinned DESC, created_at DESC")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get::<_, i64>(5)?,
            ))
        })
        .map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())
        .unwrap_or_default();
    rows.into_iter()
        .filter(|(_, kind, text, ..)| {
            q.is_empty() || (kind == "text" && text.as_deref().unwrap_or("").to_lowercase().contains(&q))
        })
        .map(|(id, kind, text, image, pinned, created_at)| ClipItem {
            id,
            kind,
            text,
            image: image.and_then(|p| std::fs::read(p).ok()).map(|b| {
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(b)
                )
            }),
            pinned,
            created_at,
        })
        .collect()
}

#[tauri::command]
pub fn clipboard_copy(id: i64) -> CmdResult<()> {
    let (kind, text, image): (String, Option<String>, Option<String>) = db()
        .query_row(
            "SELECT kind, text, image_path FROM clipboard_items WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    let mut cb = arboard::Clipboard::new().map_err(err)?;
    if kind == "text" {
        cb.set_text(text.unwrap_or_default()).map_err(err)?;
    } else {
        let img = image::open(image.ok_or("image missing")?).map_err(err)?.to_rgba8();
        let (w, h) = img.dimensions();
        cb.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(img.into_raw()),
        })
        .map_err(err)?;
    }
    ignore_seq(unsafe { GetClipboardSequenceNumber() });
    let _ = db().execute(
        "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
        params![now_ms(), id],
    );
    notify::emit("clipboard", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub fn clipboard_pin(id: i64, pinned: bool) -> CmdResult<()> {
    db().execute(
        "UPDATE clipboard_items SET pinned = ?1 WHERE id = ?2",
        params![pinned as i64, id],
    )
    .map_err(err)?;
    notify::emit("clipboard", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub fn clipboard_delete(id: i64) -> CmdResult<()> {
    let conn = db();
    if let Ok(Some(p)) = conn.query_row("SELECT image_path FROM clipboard_items WHERE id = ?1", [id], |r| {
        r.get::<_, Option<String>>(0)
    }) {
        let _ = std::fs::remove_file(p);
    }
    conn.execute("DELETE FROM clipboard_items WHERE id = ?1", [id])
        .map_err(err)?;
    drop(conn);
    notify::emit("clipboard", serde_json::json!({}));
    Ok(())
}

/// Clears everything except pinned items.
#[tauri::command]
pub fn clipboard_clear() -> CmdResult<()> {
    let conn = db();
    let images: Vec<String> = conn
        .prepare("SELECT image_path FROM clipboard_items WHERE pinned = 0 AND image_path IS NOT NULL")
        .and_then(|mut s| {
            s.query_map([], |r| r.get(0))
                .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    for p in images {
        let _ = std::fs::remove_file(p);
    }
    conn.execute("DELETE FROM clipboard_items WHERE pinned = 0", [])
        .map_err(err)?;
    drop(conn);
    notify::emit("clipboard", serde_json::json!({}));
    Ok(())
}
