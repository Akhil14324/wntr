use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::Serialize;
use tauri::AppHandle;

use crate::db::db;
use crate::drag::{self, Incoming, Payload};
use crate::notify;
use crate::util::{CmdResult, err, now_ms, sub_dir};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem {
    pub id: i64,
    pub kind: String,
    pub path: Option<String>,
    pub text: Option<String>,
    pub name: String,
    pub size: Option<i64>,
    pub ext: String,
    pub is_dir: bool,
    pub missing: bool,
    pub added_at: i64,
}

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<ShelfItem> {
    let path: Option<String> = r.get(2)?;
    let (missing, is_dir) = match &path {
        Some(p) => match std::fs::metadata(p) {
            Ok(m) => (false, m.is_dir()),
            Err(_) => (true, false),
        },
        None => (false, false),
    };
    let name: String = r.get(4)?;
    let ext = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    Ok(ShelfItem {
        id: r.get(0)?,
        kind: r.get(1)?,
        path,
        text: r.get(3)?,
        name,
        size: r.get(5)?,
        ext,
        is_dir,
        missing,
        added_at: r.get(6)?,
    })
}

pub fn list() -> Vec<ShelfItem> {
    let conn = db();
    let mut stmt = conn
        .prepare("SELECT id, kind, path, text, name, size, added_at FROM shelf_items ORDER BY added_at DESC, id DESC")
        .unwrap();
    stmt.query_map([], row_to_item)
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

fn get(ids: &[i64]) -> Vec<ShelfItem> {
    list().into_iter().filter(|i| ids.contains(&i.id)).collect()
}

fn changed() {
    notify::emit("shelf", list());
}

fn insert_file(path: &Path, owned: bool) -> rusqlite::Result<()> {
    let conn = db();
    let p = path.to_string_lossy().to_string();
    // Same file again just moves to the top.
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM shelf_items WHERE kind != 'text' AND path = ?1",
            [&p],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = exists {
        conn.execute(
            "UPDATE shelf_items SET added_at = ?1 WHERE id = ?2",
            params![now_ms(), id],
        )?;
        return Ok(());
    }
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().filter(|m| m.is_file()).map(|m| m.len() as i64);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.clone());
    let kind = if owned { "image" } else { "file" };
    conn.execute(
        "INSERT INTO shelf_items(kind, path, name, size, owned, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![kind, p, name, size, owned as i64, now_ms()],
    )?;
    Ok(())
}

fn insert_text(text: &str) -> rusqlite::Result<()> {
    let name: String = text.trim().lines().next().unwrap_or("").chars().take(80).collect();
    db().execute(
        "INSERT INTO shelf_items(kind, text, name, added_at) VALUES ('text', ?1, ?2, ?3)",
        params![text, name, now_ms()],
    )?;
    Ok(())
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "dropped-file".into()
    } else {
        cleaned
    }
}

/// Unique destination inside the shelf storage folder.
fn store_path(name: &str) -> PathBuf {
    let dir = sub_dir("shelf");
    let name = safe_file_name(name);
    let mut path = dir.join(&name);
    let stem = Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = Path::new(&name)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    path
}

pub fn add_incoming(_app: &AppHandle, items: Vec<Incoming>) {
    for item in items {
        let _ = match item {
            Incoming::File(p) => insert_file(&p, false),
            Incoming::Blob(name, bytes) => {
                let path = store_path(&name);
                match std::fs::write(&path, bytes) {
                    Ok(_) => insert_file(&path, true),
                    Err(_) => Ok(()),
                }
            }
            Incoming::Text(t) => insert_text(&t),
        };
    }
    changed();
    notify::emit("shelf-added", serde_json::json!({}));
}

#[tauri::command]
pub fn shelf_list() -> Vec<ShelfItem> {
    list()
}

#[tauri::command]
pub fn shelf_add_paths(app: AppHandle, paths: Vec<String>) {
    let files: Vec<Incoming> = paths
        .into_iter()
        .filter(|p| !p.trim().is_empty() && drag::path_exists(Path::new(p)))
        .map(|p| Incoming::File(PathBuf::from(p)))
        .collect();
    if !files.is_empty() {
        add_incoming(&app, files);
    }
}

/// "Add clipboard": files, then image, then text.
#[tauri::command]
pub fn shelf_add_clipboard(app: AppHandle) -> CmdResult<()> {
    let mut cb = arboard::Clipboard::new().map_err(err)?;
    if let Ok(files) = cb.get().file_list()
        && !files.is_empty()
    {
        add_incoming(&app, files.into_iter().map(Incoming::File).collect());
        return Ok(());
    }
    if let Ok(img) = cb.get_image()
        && let Some(png) = crate::clipboard::encode_png(&img)
    {
        let name = format!("Clipboard {}.png", chrono::Local::now().format("%Y-%m-%d %H.%M.%S"));
        add_incoming(&app, vec![Incoming::Blob(name, png)]);
        return Ok(());
    }
    match cb.get_text() {
        Ok(t) if !t.trim().is_empty() => {
            add_incoming(&app, vec![Incoming::Text(t)]);
            Ok(())
        }
        _ => Err("Clipboard has no text, image or files".into()),
    }
}

#[tauri::command]
pub fn shelf_remove(ids: Vec<i64>) -> CmdResult<()> {
    let conn = db();
    for id in ids {
        // Files we saved ourselves (browser images) are deleted with the card.
        let owned: Option<(i64, Option<String>)> = conn
            .query_row("SELECT owned, path FROM shelf_items WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        if let Some((1, Some(path))) = owned {
            let _ = std::fs::remove_file(path);
        }
        conn.execute("DELETE FROM shelf_items WHERE id = ?1", [id])
            .map_err(err)?;
    }
    drop(conn);
    changed();
    Ok(())
}

#[tauri::command]
pub fn shelf_clear() -> CmdResult<()> {
    let ids: Vec<i64> = list().iter().map(|i| i.id).collect();
    shelf_remove(ids)
}

#[tauri::command]
pub fn shelf_locate(id: i64, path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(path.clone());
    let size = std::fs::metadata(&p)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len() as i64);
    db().execute(
        "UPDATE shelf_items SET path = ?1, name = ?2, size = ?3 WHERE id = ?4",
        params![path, name, size, id],
    )
    .map_err(err)?;
    changed();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragOutcome {
    effect: String,
    removed: Vec<i64>,
}

/// Start a native drag for the given shelf items. Called while the mouse is held.
#[tauri::command]
pub async fn shelf_drag_out(app: AppHandle, ids: Vec<i64>) -> CmdResult<DragOutcome> {
    let items = get(&ids);
    if items.is_empty() {
        return Err("Nothing to drag".into());
    }
    if let Some(missing) = items.iter().find(|i| i.missing) {
        return Err(format!(
            "\"{}\" was moved or deleted. Use Locate to find it.",
            missing.name
        ));
    }
    let files: Vec<PathBuf> = items.iter().filter_map(|i| i.path.clone()).map(PathBuf::from).collect();
    let payload = if files.is_empty() {
        let text = items
            .iter()
            .filter_map(|i| i.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Payload::Text(text)
    } else {
        Payload::Files(files.clone())
    };
    let result = drag::drag_out(&app, payload).await?;
    let removed = drop_moved(&items);
    // Explorer finishes optimized moves after the drag returns; look again shortly.
    if result.effect != "copy" && result.effect != "link" {
        let items = items.clone();
        std::thread::spawn(move || {
            for delay in [700, 2500] {
                crate::util::sleep_ms(delay);
                drop_moved(&items);
            }
        });
    }
    changed();
    Ok(DragOutcome {
        effect: result.effect,
        removed,
    })
}

/// A move (Shift+drag into a folder) takes the file away: drop the stale cards.
fn drop_moved(items: &[ShelfItem]) -> Vec<i64> {
    let removed: Vec<i64> = items
        .iter()
        .filter(|i| {
            i.path
                .as_deref()
                .map(|p| !drag::path_exists(Path::new(p)))
                .unwrap_or(false)
        })
        .map(|i| i.id)
        .collect();
    if !removed.is_empty() {
        let conn = db();
        for id in &removed {
            let _ = conn.execute("DELETE FROM shelf_items WHERE id = ?1", [id]);
        }
        drop(conn);
        changed();
    }
    removed
}

/// Fallback for apps that reject drops: put the files (or text) on the clipboard.
#[tauri::command]
pub fn shelf_copy(ids: Vec<i64>) -> CmdResult<String> {
    let items = get(&ids);
    let files: Vec<PathBuf> = items
        .iter()
        .filter(|i| !i.missing)
        .filter_map(|i| i.path.clone())
        .map(PathBuf::from)
        .collect();
    if !files.is_empty() {
        drag::copy_files_to_clipboard(&files)?;
        return Ok("files".into());
    }
    let text = items
        .iter()
        .filter_map(|i| i.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err("Nothing to copy".into());
    }
    crate::clipboard::set_text(&text)?;
    Ok("text".into())
}

#[tauri::command]
pub fn shelf_copy_path(ids: Vec<i64>) -> CmdResult<()> {
    let text = get(&ids)
        .iter()
        .filter_map(|i| i.path.clone())
        .collect::<Vec<_>>()
        .join("\n");
    crate::clipboard::set_text(&text)
}

#[tauri::command]
pub fn shelf_open(app: AppHandle, id: i64) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let item = get(&[id]).pop().ok_or("Item not found")?;
    if item.missing {
        return Err(format!("\"{}\" was moved or deleted", item.name));
    }
    match item.path {
        Some(p) => app.opener().open_path(p, None::<&str>).map_err(err),
        None => crate::clipboard::set_text(item.text.as_deref().unwrap_or("")),
    }
}

#[tauri::command]
pub fn shelf_reveal(app: AppHandle, id: i64) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let item = get(&[id]).pop().ok_or("Item not found")?;
    let p = item.path.ok_or("Text cards have no file")?;
    if item.missing {
        let parent = Path::new(&p).parent().map(|p| p.to_path_buf()).ok_or("No folder")?;
        return app
            .opener()
            .open_path(parent.to_string_lossy(), None::<&str>)
            .map_err(err);
    }
    app.opener().reveal_item_in_dir(p).map_err(err)
}

/// Small thumbnail data URL for image files on the shelf.
#[tauri::command]
pub fn shelf_thumb(id: i64) -> Option<String> {
    use base64::Engine;
    let item = get(&[id]).pop()?;
    let path = item.path?;
    let ext = item.ext.as_str();
    let mime = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > 8 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_sanitised() {
        assert_eq!(safe_file_name("a/b:c?.png"), "a_b_c_.png");
        assert_eq!(safe_file_name("  "), "dropped-file");
    }
}
