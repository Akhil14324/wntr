use std::path::Path;

use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;

static DB: once_cell::sync::OnceCell<Mutex<Connection>> = once_cell::sync::OnceCell::new();

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  color TEXT NOT NULL DEFAULT 'blue',
  priority INTEGER NOT NULL DEFAULT 0,
  due_at INTEGER,
  repeat TEXT NOT NULL DEFAULT 'none',
  done INTEGER NOT NULL DEFAULT 0,
  done_at INTEGER,
  snoozed_until INTEGER,
  notified INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS task_completions (
  id INTEGER PRIMARY KEY,
  task_id INTEGER,
  title TEXT NOT NULL,
  prev_due_at INTEGER,
  at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS water_logs (
  id INTEGER PRIMARY KEY,
  ml INTEGER NOT NULL,
  at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS focus_sessions (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at INTEGER NOT NULL,
  seconds INTEGER NOT NULL,
  completed INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS shelf_items (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  path TEXT,
  text TEXT,
  name TEXT NOT NULL,
  size INTEGER,
  owned INTEGER NOT NULL DEFAULT 0,
  added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY,
  body TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(body, content='notes', content_rowid='id');
CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
  INSERT INTO notes_fts(rowid, body) VALUES (new.id, new.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, body) VALUES('delete', old.id, old.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, body) VALUES('delete', old.id, old.body);
  INSERT INTO notes_fts(rowid, body) VALUES (new.id, new.body);
END;
CREATE TABLE IF NOT EXISTS clipboard_items (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  text TEXT,
  image_path TEXT,
  hash TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS lyrics_cache (
  key TEXT PRIMARY KEY,
  synced TEXT,
  plain TEXT,
  track TEXT,
  artist TEXT,
  fetched_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS habits (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  color TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS habit_logs (
  habit_id INTEGER NOT NULL,
  day TEXT NOT NULL,
  PRIMARY KEY (habit_id, day)
);
CREATE TABLE IF NOT EXISTS calendars (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  source TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#60a5fa',
  enabled INTEGER NOT NULL DEFAULT 1,
  last_fetch INTEGER,
  last_error TEXT,
  cache TEXT
);
CREATE TABLE IF NOT EXISTS quick_launch (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  target TEXT NOT NULL,
  kind TEXT NOT NULL,
  position INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_water_at ON water_logs(at);
CREATE INDEX IF NOT EXISTS idx_focus_started ON focus_sessions(started_at);
CREATE INDEX IF NOT EXISTS idx_completions_at ON task_completions(at);
CREATE INDEX IF NOT EXISTS idx_clip_created ON clipboard_items(created_at);
"#;

/// Tables included in export/import, in dependency-safe order.
pub const TABLES: &[&str] = &[
    "tasks",
    "task_completions",
    "water_logs",
    "focus_sessions",
    "shelf_items",
    "notes",
    "clipboard_items",
    "lyrics_cache",
    "habits",
    "habit_logs",
    "calendars",
    "quick_launch",
];

pub fn open_at(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn init(dir: &Path) -> anyhow::Result<()> {
    let conn = open_at(&dir.join("wntr.db"))?;
    let _ = DB.set(Mutex::new(conn));
    Ok(())
}

pub fn db() -> MutexGuard<'static, Connection> {
    DB.get().expect("database not initialised").lock()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_and_fts_works() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_at(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO notes(body, created_at, updated_at) VALUES ('buy oat milk tomorrow', 1, 1)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'oat*'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
        conn.execute("UPDATE notes SET body = 'something else' WHERE id = 1", [])
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'oat*'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        // Re-applying the schema must be idempotent.
        conn.execute_batch(SCHEMA).unwrap();
    }
}
