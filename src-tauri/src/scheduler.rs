//! Focus timer, quick timers and every reminder (water, tasks, calendar).
//! One background thread ticks once a second.

use chrono::{Local, NaiveTime, Timelike};
use parking_lot::Mutex;
use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use crate::db::db;
use crate::util::{now_ms, sleep_ms, start_of_day_ms, today};
use crate::{notify, settings};

// ---------------------------------------------------------------------------
// Focus (Pomodoro)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FocusState {
    /// "focus" | "short" | "long"
    phase: String,
    running: bool,
    /// Phase has been started at least once (false = idle, ready to start).
    active: bool,
    ends_at: i64,
    remaining_ms: i64,
    total_ms: i64,
    /// Completed focus sessions in the current cycle.
    cycle: i64,
    cycles: i64,
    phase_started_at: i64,
    /// Milliseconds of this phase already counted before the latest resume.
    elapsed_before_ms: i64,
    resumed_at: i64,
}

fn phase_ms(phase: &str) -> i64 {
    let min = match phase {
        "short" => settings::get_i64("focus.shortMin", 5),
        "long" => settings::get_i64("focus.longMin", 15),
        _ => settings::get_i64("focus.focusMin", 25),
    };
    min.max(1) * 60_000
}

fn idle_state(phase: &str, cycle: i64) -> FocusState {
    let total = phase_ms(phase);
    FocusState {
        phase: phase.into(),
        running: false,
        active: false,
        ends_at: 0,
        remaining_ms: total,
        total_ms: total,
        cycle,
        cycles: settings::get_i64("focus.cycles", 4).max(1),
        phase_started_at: 0,
        elapsed_before_ms: 0,
        resumed_at: 0,
    }
}

static FOCUS: Mutex<Option<FocusState>> = Mutex::new(None);

fn with_focus<R>(f: impl FnOnce(&mut FocusState) -> R) -> R {
    let mut g = FOCUS.lock();
    let s = g.get_or_insert_with(|| idle_state("focus", 0));
    f(s)
}

fn emit_focus() {
    let s = with_focus(|s| s.clone());
    notify::emit("focus", &s);
}

fn log_session(kind: &str, started_at: i64, seconds: i64, completed: bool) {
    if seconds < 30 && !completed {
        return;
    }
    let _ = db().execute(
        "INSERT INTO focus_sessions(kind, started_at, ended_at, seconds, completed) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![kind, started_at, now_ms(), seconds, completed as i64],
    );
    notify::emit("stats-changed", json!({}));
}

fn start_phase(s: &mut FocusState, phase: &str) {
    let total = phase_ms(phase);
    let now = now_ms();
    s.phase = phase.into();
    s.total_ms = total;
    s.remaining_ms = total;
    s.running = true;
    s.active = true;
    s.ends_at = now + total;
    s.phase_started_at = now;
    s.elapsed_before_ms = 0;
    s.resumed_at = now;
    s.cycles = settings::get_i64("focus.cycles", 4).max(1);
}

fn elapsed_ms(s: &FocusState) -> i64 {
    s.elapsed_before_ms + if s.running { now_ms() - s.resumed_at } else { 0 }
}

/// What comes after `phase` given the cycle counter (after it was updated).
fn next_phase(phase: &str, cycle: i64, cycles: i64) -> &'static str {
    if phase == "focus" {
        if cycle >= cycles { "long" } else { "short" }
    } else {
        "focus"
    }
}

fn finish_phase(completed: bool) {
    let (ended_phase, next, auto) = with_focus(|s| {
        let ended = s.phase.clone();
        if ended == "focus" {
            log_session(
                "focus",
                s.phase_started_at,
                elapsed_ms(s).min(s.total_ms) / 1000,
                completed,
            );
            s.cycle += 1;
        } else if ended == "long" {
            s.cycle = 0;
        }
        let next = next_phase(&ended, s.cycle, s.cycles);
        let auto = ended == "focus" && settings::get_bool("focus.autoStartBreaks", true);
        let cycle = s.cycle;
        *s = idle_state(next, cycle);
        if auto {
            start_phase(s, next);
        }
        (ended, next.to_string(), auto)
    });
    if completed {
        let (title, body) = if ended_phase == "focus" {
            let label = if next == "long" { "long break" } else { "short break" };
            ("Focus session done", format!("Nice work. Time for a {label}."))
        } else {
            ("Break over", "Ready for the next focus session?".to_string())
        };
        notify::emit(
            "chime",
            json!({ "kind": if ended_phase == "focus" { "focus-end" } else { "break-end" } }),
        );
        notify::alert(
            "focus",
            title,
            &body,
            json!({ "phase": ended_phase, "next": next, "autoStarted": auto }),
        );
    }
    emit_focus();
}

fn focus_tick() {
    let done = with_focus(|s| s.running && now_ms() >= s.ends_at);
    if done {
        finish_phase(true);
    }
}

#[tauri::command]
pub fn focus_state() -> FocusState {
    with_focus(|s| {
        if s.running {
            s.remaining_ms = (s.ends_at - now_ms()).max(0);
        }
        s.clone()
    })
}

#[tauri::command]
pub fn focus_command(action: String, minutes: Option<i64>) -> FocusState {
    match action.as_str() {
        "start" => with_focus(|s| {
            let phase = s.phase.clone();
            start_phase(s, &phase);
            if let Some(m) = minutes {
                s.total_ms = m.max(1) * 60_000;
                s.remaining_ms = s.total_ms;
                s.ends_at = now_ms() + s.total_ms;
            }
        }),
        "quick" => with_focus(|s| {
            start_phase(s, "focus");
            let m = minutes.unwrap_or(25).max(1);
            s.total_ms = m * 60_000;
            s.remaining_ms = s.total_ms;
            s.ends_at = now_ms() + s.total_ms;
        }),
        "pause" => with_focus(|s| {
            if s.running {
                let now = now_ms();
                s.elapsed_before_ms += now - s.resumed_at;
                s.remaining_ms = (s.ends_at - now).max(0);
                s.running = false;
            }
        }),
        "resume" => with_focus(|s| {
            if !s.running && s.active {
                let now = now_ms();
                s.running = true;
                s.resumed_at = now;
                s.ends_at = now + s.remaining_ms;
            }
        }),
        "skip" => {
            let active = with_focus(|s| s.active);
            if active {
                finish_phase(false);
            } else {
                with_focus(|s| {
                    let next = next_phase(&s.phase.clone(), s.cycle, s.cycles);
                    let cycle = s.cycle;
                    *s = idle_state(next, cycle);
                });
            }
        }
        "reset" => with_focus(|s| {
            if s.active && s.phase == "focus" {
                log_session("focus", s.phase_started_at, elapsed_ms(s) / 1000, false);
            }
            *s = idle_state("focus", 0);
        }),
        _ => {}
    }
    emit_focus();
    focus_state()
}

// ---------------------------------------------------------------------------
// Quick timers
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QuickTimer {
    id: i64,
    label: String,
    total_ms: i64,
    ends_at: i64,
    remaining_ms: i64,
    running: bool,
}

static TIMERS: Mutex<Vec<QuickTimer>> = Mutex::new(Vec::new());

fn emit_timers() {
    notify::emit("timers", TIMERS.lock().clone());
}

fn timers_tick() {
    let now = now_ms();
    let fired: Vec<QuickTimer> = {
        let mut t = TIMERS.lock();
        let (done, keep): (Vec<_>, Vec<_>) = t.drain(..).partition(|x| x.running && x.ends_at <= now);
        *t = keep;
        done
    };
    for timer in &fired {
        notify::emit("chime", json!({ "kind": "timer" }));
        notify::alert("timer", "Timer done", &timer.label, json!({ "id": timer.id }));
    }
    if !fired.is_empty() {
        emit_timers();
    }
}

#[tauri::command]
pub fn timers_list() -> Vec<QuickTimer> {
    let now = now_ms();
    TIMERS
        .lock()
        .iter()
        .cloned()
        .map(|mut t| {
            if t.running {
                t.remaining_ms = (t.ends_at - now).max(0);
            }
            t
        })
        .collect()
}

#[tauri::command]
pub fn timer_add(seconds: i64, label: Option<String>) -> Vec<QuickTimer> {
    let total = seconds.max(1) * 1000;
    let now = now_ms();
    let label = label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| {
        if seconds % 60 == 0 {
            format!("{} min timer", seconds / 60)
        } else {
            format!("{seconds} s timer")
        }
    });
    TIMERS.lock().push(QuickTimer {
        id: now,
        label,
        total_ms: total,
        ends_at: now + total,
        remaining_ms: total,
        running: true,
    });
    emit_timers();
    timers_list()
}

#[tauri::command]
pub fn timer_command(id: i64, action: String) -> Vec<QuickTimer> {
    {
        let mut t = TIMERS.lock();
        let now = now_ms();
        match action.as_str() {
            "cancel" => t.retain(|x| x.id != id),
            "pause" => {
                if let Some(x) = t.iter_mut().find(|x| x.id == id && x.running) {
                    x.remaining_ms = (x.ends_at - now).max(0);
                    x.running = false;
                }
            }
            "resume" => {
                if let Some(x) = t.iter_mut().find(|x| x.id == id && !x.running) {
                    x.ends_at = now + x.remaining_ms;
                    x.running = true;
                }
            }
            _ => {}
        }
    }
    emit_timers();
    timers_list()
}

// ---------------------------------------------------------------------------
// Water
// ---------------------------------------------------------------------------

static LAST_WATER_REMINDER: Mutex<i64> = Mutex::new(0);

pub fn water_today_ml() -> i64 {
    db().query_row(
        "SELECT COALESCE(SUM(ml), 0) FROM water_logs WHERE at >= ?1",
        [start_of_day_ms(0)],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

fn parse_hm(s: &str, default: (u32, u32)) -> NaiveTime {
    let mut it = s.split(':');
    let h = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(default.0);
    let m = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(default.1);
    NaiveTime::from_hms_opt(h.min(23), m.min(59), 0).unwrap_or_default()
}

pub fn in_active_hours(now: NaiveTime, start: NaiveTime, end: NaiveTime) -> bool {
    if start <= end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

fn water_tick() {
    if !settings::get_bool("water.reminders", true) {
        return;
    }
    let now_t = Local::now().time().with_nanosecond(0).unwrap_or_default();
    let start = parse_hm(&settings::get_str("water.start"), (9, 0));
    let end = parse_hm(&settings::get_str("water.end"), (22, 0));
    if !in_active_hours(now_t, start, end) {
        return;
    }
    let goal = settings::get_i64("water.goal", 1500);
    let total = water_today_ml();
    if total >= goal {
        return;
    }
    let interval = settings::get_i64("water.intervalMin", 60).max(5) * 60_000;
    let last_log: i64 = db()
        .query_row("SELECT COALESCE(MAX(at), 0) FROM water_logs", [], |r| r.get(0))
        .unwrap_or(0);
    let now = now_ms();
    let mut last_rem = LAST_WATER_REMINDER.lock();
    // Count from the later of: last drink, last reminder, start of active hours today.
    let today_start = Local::now()
        .date_naive()
        .and_time(start)
        .and_local_timezone(Local)
        .earliest()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    let base = last_log.max(*last_rem).max(today_start);
    if now - base >= interval {
        *last_rem = now;
        drop(last_rem);
        notify::alert(
            "water",
            "Time for water",
            &format!("{} of {} ml today", total, goal),
            json!({ "total": total, "goal": goal }),
        );
    }
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

fn tasks_tick() {
    let now = now_ms();
    let lead = settings::get_i64("tasks.defaultReminderMin", 0).max(0) * 60_000;
    let due: Vec<(i64, String, i64)> = {
        let conn = db();
        let mut stmt = match conn.prepare(
            "SELECT id, title, COALESCE(snoozed_until, due_at - ?2) AS fire FROM tasks
             WHERE done = 0 AND notified = 0 AND due_at IS NOT NULL AND COALESCE(snoozed_until, due_at - ?2) <= ?1",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        stmt.query_map(params![now, lead], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    for (id, title, _) in due {
        let _ = db().execute("UPDATE tasks SET notified = 1 WHERE id = ?1", [id]);
        notify::alert("task", &title, "Task due now", json!({ "id": id }));
    }
}

// ---------------------------------------------------------------------------

pub fn start() {
    crate::util::spawn_loop("scheduler", || {
        let mut tick: u64 = 0;
        let mut day = today();
        loop {
            focus_tick();
            timers_tick();
            if tick.is_multiple_of(10) {
                tasks_tick();
                crate::calendar::alerts_tick();
            }
            if tick % 30 == 5 {
                water_tick();
            }
            if tick.is_multiple_of(30) {
                let d = today();
                if d != day {
                    day = d;
                    notify::emit("day-changed", json!({ "day": day }));
                }
            }
            tick = tick.wrapping_add(1);
            sleep_ms(1000);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_hours_wrap_midnight() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert!(in_active_hours(t(10, 0), t(9, 0), t(22, 0)));
        assert!(!in_active_hours(t(23, 0), t(9, 0), t(22, 0)));
        assert!(in_active_hours(t(23, 0), t(22, 0), t(6, 0)));
        assert!(in_active_hours(t(1, 0), t(22, 0), t(6, 0)));
        assert!(!in_active_hours(t(12, 0), t(22, 0), t(6, 0)));
    }

    #[test]
    fn pomodoro_order() {
        assert_eq!(next_phase("focus", 1, 4), "short");
        assert_eq!(next_phase("focus", 4, 4), "long");
        assert_eq!(next_phase("short", 1, 4), "focus");
        assert_eq!(next_phase("long", 0, 4), "focus");
    }
}
