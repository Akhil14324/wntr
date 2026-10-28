//! Local data pages: tasks, water, notes, habits, quick launcher, stats and
//! export/import. Everything lives in SQLite under %APPDATA%\WNTR.

use std::collections::{BTreeMap, HashSet};

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Weekday};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::AppHandle;

use crate::db::{TABLES, db};
use crate::util::{CmdResult, err, local_day, now_ms, start_of_day_ms, today};
use crate::{notify, settings};

fn changed(what: &str) {
    notify::emit(what, json!({}));
    notify::emit("stats-changed", json!({}));
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    id: i64,
    title: String,
    notes: String,
    color: String,
    priority: i64,
    due_at: Option<i64>,
    repeat: String,
    done: bool,
    snoozed_until: Option<i64>,
    created_at: i64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    id: Option<i64>,
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    priority: i64,
    due_at: Option<i64>,
    #[serde(default)]
    repeat: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    id: i64,
    task_id: Option<i64>,
    title: String,
    at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLists {
    open: Vec<Task>,
    done: Vec<Completion>,
    completed_today: i64,
}

fn row_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        color: r.get(3)?,
        priority: r.get(4)?,
        due_at: r.get(5)?,
        repeat: r.get(6)?,
        done: r.get::<_, i64>(7)? != 0,
        snoozed_until: r.get(8)?,
        created_at: r.get(9)?,
    })
}

const TASK_COLS: &str = "id, title, notes, color, priority, due_at, repeat, done, snoozed_until, created_at";

/// Next occurrence of a repeating task strictly after `now`, keeping wall-clock time.
pub fn next_due(due: i64, repeat: &str, now: i64) -> Option<i64> {
    let step = |d: chrono::NaiveDateTime| -> chrono::NaiveDateTime {
        match repeat {
            "daily" => d + Duration::days(1),
            "weekly" => d + Duration::days(7),
            "weekdays" => {
                let mut x = d + Duration::days(1);
                while matches!(x.weekday(), Weekday::Sat | Weekday::Sun) {
                    x += Duration::days(1);
                }
                x
            }
            _ => d,
        }
    };
    if !matches!(repeat, "daily" | "weekly" | "weekdays") {
        return None;
    }
    let mut d = Local.timestamp_millis_opt(due).single()?.naive_local();
    for _ in 0..3700 {
        d = step(d);
        let ms = Local.from_local_datetime(&d).earliest()?.timestamp_millis();
        if ms > now {
            return Some(ms);
        }
    }
    None
}

#[tauri::command]
pub fn tasks_list() -> TaskLists {
    let conn = db();
    let open = conn
        .prepare(&format!(
            "SELECT {TASK_COLS} FROM tasks WHERE done = 0
             ORDER BY due_at IS NULL, due_at, priority DESC, created_at"
        ))
        .and_then(|mut s| s.query_map([], row_task).map(|r| r.filter_map(Result::ok).collect()))
        .unwrap_or_default();
    let done = conn
        .prepare("SELECT id, task_id, title, at FROM task_completions ORDER BY at DESC LIMIT 100")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(Completion {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    title: r.get(2)?,
                    at: r.get(3)?,
                })
            })
            .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    let completed_today = conn
        .query_row(
            "SELECT count(*) FROM task_completions WHERE at >= ?1",
            [start_of_day_ms(0)],
            |r| r.get(0),
        )
        .unwrap_or(0);
    TaskLists {
        open,
        done,
        completed_today,
    }
}

#[tauri::command]
pub fn task_save(task: TaskInput) -> CmdResult<i64> {
    let title = task.title.trim().to_string();
    if title.is_empty() {
        return Err("Task needs a title".into());
    }
    let color = task.color.unwrap_or_else(|| "blue".into());
    let repeat = task.repeat.unwrap_or_else(|| "none".into());
    let conn = db();
    let id = match task.id {
        Some(id) => {
            conn.execute(
                "UPDATE tasks SET title=?1, notes=?2, color=?3, priority=?4, due_at=?5, repeat=?6,
                 notified=0, snoozed_until=NULL WHERE id=?7",
                params![title, task.notes, color, task.priority, task.due_at, repeat, id],
            )
            .map_err(err)?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO tasks(title, notes, color, priority, due_at, repeat, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![title, task.notes, color, task.priority, task.due_at, repeat, now_ms()],
            )
            .map_err(err)?;
            conn.last_insert_rowid()
        }
    };
    drop(conn);
    changed("tasks");
    Ok(id)
}

#[tauri::command]
pub fn task_delete(id: i64) -> CmdResult<()> {
    db().execute("DELETE FROM tasks WHERE id = ?1", [id]).map_err(err)?;
    changed("tasks");
    Ok(())
}

/// Complete a task. Repeating tasks move to their next occurrence instead.
#[tauri::command]
pub fn task_complete(id: i64) -> CmdResult<i64> {
    let conn = db();
    let (title, due, repeat): (String, Option<i64>, String) = conn
        .query_row("SELECT title, due_at, repeat FROM tasks WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(err)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO task_completions(task_id, title, prev_due_at, at) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, due, now],
    )
    .map_err(err)?;
    let completion = conn.last_insert_rowid();
    match due.and_then(|d| next_due(d, &repeat, now)) {
        Some(next) => conn.execute(
            "UPDATE tasks SET due_at = ?1, notified = 0, snoozed_until = NULL WHERE id = ?2",
            params![next, id],
        ),
        None => conn.execute(
            "UPDATE tasks SET done = 1, done_at = ?1 WHERE id = ?2",
            params![now, id],
        ),
    }
    .map_err(err)?;
    drop(conn);
    changed("tasks");
    Ok(completion)
}

#[tauri::command]
pub fn task_undo_complete(completion_id: i64) -> CmdResult<()> {
    let conn = db();
    let (task_id, prev_due): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT task_id, prev_due_at FROM task_completions WHERE id = ?1",
            [completion_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if let Some(tid) = task_id {
        conn.execute(
            "UPDATE tasks SET done = 0, done_at = NULL, due_at = COALESCE(?1, due_at), notified = 1 WHERE id = ?2",
            params![prev_due, tid],
        )
        .map_err(err)?;
    }
    conn.execute("DELETE FROM task_completions WHERE id = ?1", [completion_id])
        .map_err(err)?;
    drop(conn);
    changed("tasks");
    Ok(())
}

#[tauri::command]
pub fn task_snooze(id: i64, minutes: i64) -> CmdResult<()> {
    let until = now_ms() + minutes.max(1) * 60_000;
    db().execute(
        "UPDATE tasks SET snoozed_until = ?1, notified = 0 WHERE id = ?2",
        params![until, id],
    )
    .map_err(err)?;
    changed("tasks");
    Ok(())
}

// ---------------------------------------------------------------------------
// Water
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterSummary {
    today: i64,
    goal: i64,
    logs: Vec<(i64, i64, i64)>,
    /// Last 30 days (oldest first): (day, ml).
    days: Vec<(String, i64)>,
}

fn water_by_day(days: i64) -> Vec<(String, i64)> {
    let since = start_of_day_ms(days - 1);
    let mut map: BTreeMap<String, i64> = BTreeMap::new();
    for i in (0..days).rev() {
        map.insert(local_day(start_of_day_ms(i)), 0);
    }
    let conn = db();
    if let Ok(mut s) = conn.prepare("SELECT ml, at FROM water_logs WHERE at >= ?1")
        && let Ok(rows) = s.query_map([since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
    {
        for (ml, at) in rows.flatten() {
            *map.entry(local_day(at)).or_insert(0) += ml;
        }
    }
    map.into_iter().collect()
}

#[tauri::command]
pub fn water_summary() -> WaterSummary {
    let logs = db()
        .prepare("SELECT id, ml, at FROM water_logs WHERE at >= ?1 ORDER BY at DESC")
        .and_then(|mut s| {
            s.query_map([start_of_day_ms(0)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    WaterSummary {
        today: crate::scheduler::water_today_ml(),
        goal: settings::get_i64("water.goal", 1500),
        logs,
        days: water_by_day(30),
    }
}

#[tauri::command]
pub fn water_add(ml: i64) -> CmdResult<WaterSummary> {
    if ml <= 0 || ml > 5000 {
        return Err("Amount must be between 1 and 5000 ml".into());
    }
    let before = crate::scheduler::water_today_ml();
    db().execute("INSERT INTO water_logs(ml, at) VALUES (?1, ?2)", params![ml, now_ms()])
        .map_err(err)?;
    changed("water");
    let s = water_summary();
    if before < s.goal && s.today >= s.goal {
        notify::emit("water-goal", json!({ "goal": s.goal }));
    }
    Ok(s)
}

#[tauri::command]
pub fn water_undo() -> CmdResult<WaterSummary> {
    db().execute(
        "DELETE FROM water_logs WHERE id = (SELECT id FROM water_logs ORDER BY at DESC LIMIT 1)",
        [],
    )
    .map_err(err)?;
    changed("water");
    Ok(water_summary())
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    id: i64,
    body: String,
    pinned: bool,
    created_at: i64,
    updated_at: i64,
}

fn row_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        body: r.get(1)?,
        pinned: r.get::<_, i64>(2)? != 0,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
    })
}

/// Turn free text into an FTS5 prefix query: every word must match.
pub fn fts_query(q: &str) -> String {
    q.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
pub fn notes_list(query: Option<String>) -> Vec<Note> {
    let conn = db();
    let q = fts_query(query.as_deref().unwrap_or(""));
    let res = if q.is_empty() {
        conn.prepare("SELECT id, body, pinned, created_at, updated_at FROM notes ORDER BY pinned DESC, updated_at DESC")
            .and_then(|mut s| s.query_map([], row_note).map(|r| r.filter_map(Result::ok).collect()))
    } else {
        conn.prepare(
            "SELECT n.id, n.body, n.pinned, n.created_at, n.updated_at FROM notes n
             JOIN notes_fts f ON f.rowid = n.id WHERE notes_fts MATCH ?1
             ORDER BY n.pinned DESC, rank",
        )
        .and_then(|mut s| s.query_map([q], row_note).map(|r| r.filter_map(Result::ok).collect()))
    };
    res.unwrap_or_default()
}

#[tauri::command]
pub fn note_add(body: String) -> CmdResult<i64> {
    if body.trim().is_empty() {
        return Err("Note is empty".into());
    }
    let now = now_ms();
    let conn = db();
    conn.execute(
        "INSERT INTO notes(body, created_at, updated_at) VALUES (?1, ?2, ?2)",
        params![body, now],
    )
    .map_err(err)?;
    let id = conn.last_insert_rowid();
    drop(conn);
    changed("notes");
    Ok(id)
}

#[tauri::command]
pub fn note_update(id: i64, body: String) -> CmdResult<()> {
    db().execute(
        "UPDATE notes SET body = ?1, updated_at = ?2 WHERE id = ?3",
        params![body, now_ms(), id],
    )
    .map_err(err)?;
    changed("notes");
    Ok(())
}

#[tauri::command]
pub fn note_pin(id: i64, pinned: bool) -> CmdResult<()> {
    db().execute("UPDATE notes SET pinned = ?1 WHERE id = ?2", params![pinned as i64, id])
        .map_err(err)?;
    changed("notes");
    Ok(())
}

#[tauri::command]
pub fn note_delete(id: i64) -> CmdResult<()> {
    db().execute("DELETE FROM notes WHERE id = ?1", [id]).map_err(err)?;
    changed("notes");
    Ok(())
}

fn note_file_name(n: &Note) -> String {
    let first: String = n.body.lines().next().unwrap_or("note").chars().take(40).collect();
    let safe: String = first
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim().to_string();
    let date = Local
        .timestamp_millis_opt(n.created_at)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    format!("{date} {} ({}).md", if safe.is_empty() { "note" } else { &safe }, n.id)
}

/// Export notes (all when `ids` is empty) as Markdown files into `dir`.
#[tauri::command]
pub fn notes_export(ids: Option<Vec<i64>>, dir: Option<String>) -> CmdResult<Vec<String>> {
    let dir = dir
        .filter(|d| !d.is_empty())
        .or_else(|| Some(settings::get_str("notes.exportDir")).filter(|d| !d.is_empty()))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|p| std::path::PathBuf::from(p).join("Documents").join("WNTR Notes"))
                .unwrap_or_else(|| crate::util::sub_dir("notes-export"))
        });
    std::fs::create_dir_all(&dir).map_err(err)?;
    let ids = ids.unwrap_or_default();
    let mut out = Vec::new();
    for n in notes_list(None)
        .into_iter()
        .filter(|n| ids.is_empty() || ids.contains(&n.id))
    {
        let path = dir.join(note_file_name(&n));
        std::fs::write(&path, &n.body).map_err(err)?;
        out.push(path.to_string_lossy().to_string());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Habits
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Habit {
    id: i64,
    name: String,
    color: String,
    days: Vec<String>,
    streak: i64,
    done_today: bool,
}

/// Consecutive days ending today (or yesterday, if today isn't checked yet).
pub fn streak(days: &HashSet<String>, today: NaiveDate) -> i64 {
    let mut d = today;
    if !days.contains(&d.format("%Y-%m-%d").to_string()) {
        d -= Duration::days(1);
    }
    let mut n = 0;
    while days.contains(&d.format("%Y-%m-%d").to_string()) {
        n += 1;
        d -= Duration::days(1);
    }
    n
}

#[tauri::command]
pub fn habits_list() -> Vec<Habit> {
    let conn = db();
    let habits: Vec<(i64, String, String)> = conn
        .prepare("SELECT id, name, color FROM habits ORDER BY created_at")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    let t = today();
    let today_date = Local::now().date_naive();
    habits
        .into_iter()
        .map(|(id, name, color)| {
            let all: HashSet<String> = conn
                .prepare("SELECT day FROM habit_logs WHERE habit_id = ?1")
                .and_then(|mut s| {
                    s.query_map([id], |r| r.get(0))
                        .map(|r| r.filter_map(Result::ok).collect())
                })
                .unwrap_or_default();
            let mut recent: Vec<String> = all
                .iter()
                .filter(|d| d.as_str() >= local_day(start_of_day_ms(34)).as_str())
                .cloned()
                .collect();
            recent.sort();
            Habit {
                id,
                name,
                color,
                streak: streak(&all, today_date),
                done_today: all.contains(&t),
                days: recent,
            }
        })
        .collect()
}

#[tauri::command]
pub fn habit_save(id: Option<i64>, name: String, color: String) -> CmdResult<()> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Habit needs a name".into());
    }
    let conn = db();
    match id {
        Some(id) => conn.execute(
            "UPDATE habits SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, color, id],
        ),
        None => conn.execute(
            "INSERT INTO habits(name, color, created_at) VALUES (?1, ?2, ?3)",
            params![name, color, now_ms()],
        ),
    }
    .map_err(err)?;
    drop(conn);
    changed("habits");
    Ok(())
}

#[tauri::command]
pub fn habit_delete(id: i64) -> CmdResult<()> {
    let conn = db();
    conn.execute("DELETE FROM habit_logs WHERE habit_id = ?1", [id])
        .map_err(err)?;
    conn.execute("DELETE FROM habits WHERE id = ?1", [id]).map_err(err)?;
    drop(conn);
    changed("habits");
    Ok(())
}

#[tauri::command]
pub fn habit_toggle(id: i64, day: Option<String>) -> CmdResult<()> {
    let day = day.unwrap_or_else(today);
    let conn = db();
    let removed = conn
        .execute(
            "DELETE FROM habit_logs WHERE habit_id = ?1 AND day = ?2",
            params![id, day],
        )
        .map_err(err)?;
    if removed == 0 {
        conn.execute(
            "INSERT INTO habit_logs(habit_id, day) VALUES (?1, ?2)",
            params![id, day],
        )
        .map_err(err)?;
    }
    drop(conn);
    changed("habits");
    Ok(())
}

// ---------------------------------------------------------------------------
// Quick launcher
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchItem {
    id: i64,
    name: String,
    target: String,
    kind: String,
}

#[tauri::command]
pub fn launcher_list() -> Vec<LaunchItem> {
    db().prepare("SELECT id, name, target, kind FROM quick_launch ORDER BY position, id")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(LaunchItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    target: r.get(2)?,
                    kind: r.get(3)?,
                })
            })
            .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default()
}

pub fn launch_kind(target: &str) -> &'static str {
    let t = target.trim().to_lowercase();
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("mailto:") {
        "url"
    } else if std::path::Path::new(target).is_dir() {
        "folder"
    } else {
        "app"
    }
}

#[tauri::command]
pub fn launcher_add(name: Option<String>, target: String) -> CmdResult<()> {
    let mut target = target.trim().trim_matches('"').to_string();
    if target.is_empty() {
        return Err("Enter a path or URL".into());
    }
    if !target.contains("://")
        && !target.contains(":\\")
        && !target.starts_with("\\\\")
        && target.contains('.')
        && !target.contains(' ')
        && !target.starts_with("mailto:")
    {
        target = format!("https://{target}");
    }
    let kind = launch_kind(&target);
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        if kind == "url" {
            target
                .split("://")
                .nth(1)
                .unwrap_or(&target)
                .split('/')
                .next()
                .unwrap_or(&target)
                .trim_start_matches("www.")
                .to_string()
        } else {
            std::path::Path::new(&target)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(target.clone())
        }
    });
    let conn = db();
    let pos: i64 = conn
        .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM quick_launch", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO quick_launch(name, target, kind, position) VALUES (?1, ?2, ?3, ?4)",
        params![name, target, kind, pos],
    )
    .map_err(err)?;
    drop(conn);
    changed("launcher");
    Ok(())
}

#[tauri::command]
pub fn launcher_remove(id: i64) -> CmdResult<()> {
    db().execute("DELETE FROM quick_launch WHERE id = ?1", [id])
        .map_err(err)?;
    changed("launcher");
    Ok(())
}

#[tauri::command]
pub fn launcher_move(id: i64, delta: i64) -> CmdResult<()> {
    let mut items = launcher_list();
    let Some(i) = items.iter().position(|x| x.id == id) else {
        return Ok(());
    };
    let j = (i as i64 + delta).clamp(0, items.len() as i64 - 1) as usize;
    let item = items.remove(i);
    items.insert(j, item);
    let conn = db();
    for (pos, it) in items.iter().enumerate() {
        let _ = conn.execute(
            "UPDATE quick_launch SET position = ?1 WHERE id = ?2",
            params![pos as i64, it.id],
        );
    }
    drop(conn);
    changed("launcher");
    Ok(())
}

#[tauri::command]
pub fn open_target(app: AppHandle, target: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if launch_kind(&target) == "url" {
        app.opener().open_url(target, None::<&str>).map_err(err)
    } else {
        app.opener().open_path(target, None::<&str>).map_err(err)
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    day: String,
    focus_min: f64,
    sessions: i64,
    water_ml: i64,
    tasks: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    days: Vec<DayStat>,
    focus_min: f64,
    sessions: i64,
    best_day: Option<String>,
    best_day_min: f64,
    focus_streak: i64,
    water_avg: f64,
    water_goal_days: i64,
    water_streak: i64,
    tasks_done: i64,
    tasks_per_week: f64,
}

fn first_record_ms(conn: &Connection) -> i64 {
    let q = "SELECT MIN(t) FROM (SELECT MIN(started_at) t FROM focus_sessions UNION ALL SELECT MIN(at) FROM water_logs UNION ALL SELECT MIN(at) FROM task_completions)";
    conn.query_row(q, [], |r| r.get::<_, Option<i64>>(0))
        .ok()
        .flatten()
        .unwrap_or_else(now_ms)
}

fn day_stats(range_days: i64) -> Vec<DayStat> {
    let conn = db();
    let days = if range_days <= 0 {
        let first = first_record_ms(&conn);
        ((now_ms() - first) / 86_400_000 + 1).clamp(1, 3650)
    } else {
        range_days
    };
    let since = start_of_day_ms(days - 1);
    let mut map: BTreeMap<String, DayStat> = BTreeMap::new();
    for i in (0..days).rev() {
        let day = local_day(start_of_day_ms(i));
        map.insert(
            day.clone(),
            DayStat {
                day,
                ..Default::default()
            },
        );
    }
    if let Ok(mut s) =
        conn.prepare("SELECT started_at, seconds FROM focus_sessions WHERE kind = 'focus' AND started_at >= ?1")
        && let Ok(rows) = s.query_map([since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
    {
        for (at, secs) in rows.flatten() {
            if let Some(d) = map.get_mut(&local_day(at)) {
                d.focus_min += secs as f64 / 60.0;
                d.sessions += 1;
            }
        }
    }
    if let Ok(mut s) = conn.prepare("SELECT at, ml FROM water_logs WHERE at >= ?1")
        && let Ok(rows) = s.query_map([since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
    {
        for (at, ml) in rows.flatten() {
            if let Some(d) = map.get_mut(&local_day(at)) {
                d.water_ml += ml;
            }
        }
    }
    if let Ok(mut s) = conn.prepare("SELECT at FROM task_completions WHERE at >= ?1")
        && let Ok(rows) = s.query_map([since], |r| r.get::<_, i64>(0))
    {
        for at in rows.flatten() {
            if let Some(d) = map.get_mut(&local_day(at)) {
                d.tasks += 1;
            }
        }
    }
    map.into_values()
        .map(|mut d| {
            d.focus_min = (d.focus_min * 10.0).round() / 10.0;
            d
        })
        .collect()
}

fn trailing_streak(days: &[DayStat], hit: impl Fn(&DayStat) -> bool) -> i64 {
    let mut it = days.iter().rev().peekable();
    // Today not reached yet doesn't break the streak.
    if let Some(d) = it.peek()
        && !hit(d)
    {
        it.next();
    }
    it.take_while(|d| hit(d)).count() as i64
}

#[tauri::command]
pub fn stats_get(range_days: i64) -> Stats {
    let days = day_stats(range_days);
    let streak_days = day_stats(400);
    let goal = settings::get_i64("water.goal", 1500);
    let focus_min: f64 = days.iter().map(|d| d.focus_min).sum();
    let sessions = days.iter().map(|d| d.sessions).sum();
    let best = days
        .iter()
        .filter(|d| d.focus_min > 0.0)
        .max_by(|a, b| a.focus_min.partial_cmp(&b.focus_min).unwrap());
    let water_days: Vec<&DayStat> = days.iter().filter(|d| d.water_ml > 0).collect();
    let tasks_done: i64 = days.iter().map(|d| d.tasks).sum();
    Stats {
        focus_min: (focus_min * 10.0).round() / 10.0,
        sessions,
        best_day: best.map(|d| d.day.clone()),
        best_day_min: best.map(|d| d.focus_min).unwrap_or(0.0),
        focus_streak: trailing_streak(&streak_days, |d| d.sessions > 0),
        water_avg: if water_days.is_empty() {
            0.0
        } else {
            water_days.iter().map(|d| d.water_ml).sum::<i64>() as f64 / water_days.len() as f64
        },
        water_goal_days: days.iter().filter(|d| d.water_ml >= goal).count() as i64,
        water_streak: trailing_streak(&streak_days, |d| d.water_ml >= goal),
        tasks_done,
        tasks_per_week: tasks_done as f64 / (days.len().max(1) as f64 / 7.0).max(1.0),
        days,
    }
}

#[tauri::command]
pub fn stats_export_csv(area: String, range_days: i64, path: String) -> CmdResult<()> {
    let days = day_stats(range_days);
    let mut csv = String::new();
    match area.as_str() {
        "focus" => {
            csv.push_str("day,focus_minutes,sessions\n");
            for d in &days {
                csv.push_str(&format!("{},{},{}\n", d.day, d.focus_min, d.sessions));
            }
        }
        "water" => {
            csv.push_str("day,water_ml\n");
            for d in &days {
                csv.push_str(&format!("{},{}\n", d.day, d.water_ml));
            }
        }
        "tasks" => {
            csv.push_str("day,tasks_completed\n");
            for d in &days {
                csv.push_str(&format!("{},{}\n", d.day, d.tasks));
            }
        }
        _ => {
            csv.push_str("day,focus_minutes,sessions,water_ml,tasks_completed\n");
            for d in &days {
                csv.push_str(&format!(
                    "{},{},{},{},{}\n",
                    d.day, d.focus_min, d.sessions, d.water_ml, d.tasks
                ));
            }
        }
    }
    std::fs::write(path, csv).map_err(err)
}

// ---------------------------------------------------------------------------
// Export / import / reset
// ---------------------------------------------------------------------------

fn table_rows(conn: &Connection, table: &str) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table}"))?;
    let cols: Vec<String> = stmt.column_names().map(|c| c.to_string()).collect();
    let rows = stmt.query_map([], |r| {
        let mut obj = serde_json::Map::new();
        for (i, c) in cols.iter().enumerate() {
            let v = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(f) => json!(f),
                ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                ValueRef::Blob(_) => Value::Null,
            };
            obj.insert(c.clone(), v);
        }
        Ok(Value::Object(obj))
    })?;
    rows.collect()
}

pub fn export_value() -> CmdResult<Value> {
    let conn = db();
    let mut tables = serde_json::Map::new();
    for t in TABLES {
        tables.insert(t.to_string(), Value::Array(table_rows(&conn, t).map_err(err)?));
    }
    Ok(json!({
        "app": "WNTR",
        "version": env!("CARGO_PKG_VERSION"),
        "exportedAt": now_ms(),
        "settings": settings::all(),
        "tables": tables,
    }))
}

pub fn import_value(conn: &mut Connection, v: &Value) -> CmdResult<()> {
    let tables = v
        .get("tables")
        .and_then(|t| t.as_object())
        .ok_or("Not a WNTR export file")?;
    let tx = conn.transaction().map_err(err)?;
    for t in TABLES {
        tx.execute(&format!("DELETE FROM {t}"), []).map_err(err)?;
        let Some(rows) = tables.get(*t).and_then(|r| r.as_array()) else {
            continue;
        };
        for row in rows {
            let Some(obj) = row.as_object() else { continue };
            let cols: Vec<&String> = obj
                .keys()
                .filter(|k| k.chars().all(|c| c.is_alphanumeric() || c == '_'))
                .collect();
            if cols.is_empty() {
                continue;
            }
            let sql = format!(
                "INSERT INTO {t}({}) VALUES ({})",
                cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(","),
                (1..=cols.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
            );
            let vals: Vec<rusqlite::types::Value> = cols
                .iter()
                .map(|c| match &obj[*c] {
                    Value::Null => rusqlite::types::Value::Null,
                    Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
                    Value::Number(n) => n
                        .as_i64()
                        .map(rusqlite::types::Value::Integer)
                        .unwrap_or_else(|| rusqlite::types::Value::Real(n.as_f64().unwrap_or(0.0))),
                    Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                    other => rusqlite::types::Value::Text(other.to_string()),
                })
                .collect();
            tx.execute(&sql, rusqlite::params_from_iter(vals)).map_err(err)?;
        }
    }
    tx.execute("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", [])
        .map_err(err)?;
    tx.commit().map_err(err)
}

#[tauri::command]
pub fn data_export(path: String) -> CmdResult<()> {
    let v = export_value()?;
    std::fs::write(path, serde_json::to_string_pretty(&v).map_err(err)?).map_err(err)
}

#[tauri::command]
pub fn data_import(app: AppHandle, path: String) -> CmdResult<()> {
    let text = std::fs::read_to_string(path).map_err(err)?;
    let v: Value = serde_json::from_str(&text).map_err(err)?;
    import_value(&mut db(), &v)?;
    if let Some(s) = v.get("settings") {
        settings::replace_all(&app, s.clone());
    }
    notify::emit("data-imported", json!({}));
    crate::shelf::shelf_list();
    Ok(())
}

#[tauri::command]
pub fn data_reset(app: AppHandle) -> CmdResult<()> {
    {
        let conn = db();
        for t in TABLES {
            conn.execute(&format!("DELETE FROM {t}"), []).map_err(err)?;
        }
        let _ = conn.execute("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", []);
    }
    settings::replace_all(&app, settings::defaults());
    notify::emit("data-imported", json!({}));
    Ok(())
}

#[tauri::command]
pub fn open_data_folder(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(crate::util::data_dir().to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(y: i32, mo: u32, d: u32, h: u32) -> i64 {
        Local
            .with_ymd_and_hms(y, mo, d, h, 0, 0)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn repeat_rules() {
        let fri = ms(2026, 10, 23, 9); // Friday
        assert_eq!(next_due(fri, "daily", fri), Some(ms(2026, 10, 24, 9)));
        assert_eq!(next_due(fri, "weekdays", fri), Some(ms(2026, 10, 26, 9)));
        assert_eq!(next_due(fri, "weekly", fri), Some(ms(2026, 10, 30, 9)));
        // Overdue by several days jumps past now.
        assert_eq!(next_due(fri, "daily", ms(2026, 10, 27, 12)), Some(ms(2026, 10, 28, 9)));
        assert_eq!(next_due(fri, "none", fri), None);
    }

    #[test]
    fn fts_query_builds_prefix_terms() {
        assert_eq!(fts_query("oat milk"), "\"oat\"* \"milk\"*");
        assert_eq!(fts_query("  \"weird\" ** "), "\"weird\"*");
        assert_eq!(fts_query(""), "");
    }

    #[test]
    fn habit_streaks() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let set: HashSet<String> = ["2026-10-27", "2026-10-26", "2026-10-24"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(streak(&set, today), 2);
        let mut set2 = set.clone();
        set2.insert("2026-10-28".into());
        assert_eq!(streak(&set2, today), 3);
    }

    #[test]
    fn export_import_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open_at(&dir.path().join("a.db")).unwrap();
        conn.execute(
            "INSERT INTO notes(body, created_at, updated_at) VALUES ('hello world', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO water_logs(ml, at) VALUES (500, 5)", [])
            .unwrap();
        let mut tables = serde_json::Map::new();
        for t in TABLES {
            tables.insert(t.to_string(), Value::Array(table_rows(&conn, t).unwrap()));
        }
        let v = json!({ "tables": tables });
        let mut conn2 = crate::db::open_at(&dir.path().join("b.db")).unwrap();
        import_value(&mut conn2, &v).unwrap();
        let n: i64 = conn2
            .query_row(
                "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'hello*'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        let ml: i64 = conn2.query_row("SELECT ml FROM water_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(ml, 500);
        let _ = &mut conn;
    }
}
