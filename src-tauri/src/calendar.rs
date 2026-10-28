//! Calendars from local .ics files or secret iCal URLs (Google, Outlook, Apple).
//! Nothing is sent anywhere except a GET to the URL you add.

use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use parking_lot::Mutex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::db;
use crate::util::{CmdResult, err, http, now_ms, sleep_ms};
use crate::{notify, settings};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    pub id: String,
    pub calendar_id: i64,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
    pub join_url: Option<String>,
    pub color: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calendar {
    id: i64,
    name: String,
    source: String,
    color: String,
    enabled: bool,
    last_fetch: Option<i64>,
    last_error: Option<String>,
}

static EVENTS: Mutex<Vec<CalEvent>> = Mutex::new(Vec::new());
static ALERTED: once_cell::sync::Lazy<Mutex<HashSet<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashSet::new()));

// ---------------------------------------------------------------------------
// Time parsing
// ---------------------------------------------------------------------------

fn windows_tz(name: &str) -> Option<&'static str> {
    Some(match name {
        "Pacific Standard Time" => "America/Los_Angeles",
        "Mountain Standard Time" => "America/Denver",
        "US Mountain Standard Time" => "America/Phoenix",
        "Central Standard Time" => "America/Chicago",
        "Eastern Standard Time" => "America/New_York",
        "Atlantic Standard Time" => "America/Halifax",
        "Alaskan Standard Time" => "America/Anchorage",
        "Hawaiian Standard Time" => "Pacific/Honolulu",
        "Central Standard Time (Mexico)" | "Mexico Standard Time" => "America/Mexico_City",
        "SA Pacific Standard Time" => "America/Bogota",
        "Argentina Standard Time" => "America/Argentina/Buenos_Aires",
        "E. South America Standard Time" => "America/Sao_Paulo",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Romance Standard Time" => "Europe/Paris",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "FLE Standard Time" => "Europe/Kiev",
        "GTB Standard Time" => "Europe/Bucharest",
        "Turkey Standard Time" => "Europe/Istanbul",
        "Russian Standard Time" => "Europe/Moscow",
        "Israel Standard Time" => "Asia/Jerusalem",
        "Egypt Standard Time" => "Africa/Cairo",
        "South Africa Standard Time" => "Africa/Johannesburg",
        "Arabian Standard Time" => "Asia/Dubai",
        "Pakistan Standard Time" => "Asia/Karachi",
        "India Standard Time" => "Asia/Kolkata",
        "Bangladesh Standard Time" => "Asia/Dhaka",
        "SE Asia Standard Time" => "Asia/Jakarta",
        "Singapore Standard Time" => "Asia/Singapore",
        "China Standard Time" => "Asia/Shanghai",
        "Taipei Standard Time" => "Asia/Taipei",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Korea Standard Time" => "Asia/Seoul",
        "W. Australia Standard Time" => "Australia/Perth",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "New Zealand Standard Time" => "Pacific/Auckland",
        "UTC" | "Coordinated Universal Time" => "UTC",
        _ => return None,
    })
}

fn parse_tz(name: &str) -> Option<chrono_tz::Tz> {
    let name = name.trim().trim_matches('"');
    if let Ok(tz) = name.parse::<chrono_tz::Tz>() {
        return Some(tz);
    }
    if let Some(iana) = windows_tz(name) {
        return iana.parse().ok();
    }
    // e.g. "/mozilla.org/20050126_1/America/New_York"
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
    for n in (1..=3.min(parts.len())).rev() {
        if let Ok(tz) = parts[parts.len() - n..].join("/").parse::<chrono_tz::Tz>() {
            return Some(tz);
        }
    }
    None
}

#[derive(Clone, Debug)]
struct IcsTime {
    utc: DateTime<Utc>,
    all_day: bool,
    /// Original wall time + zone, used to expand recurrences.
    naive: NaiveDateTime,
    tz: Option<chrono_tz::Tz>,
    is_utc: bool,
}

fn param<'a>(p: &'a ical::property::Property, key: &str) -> Option<&'a str> {
    p.params
        .as_ref()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.first().map(|s| s.as_str()))
}

fn parse_time(value: &str, tzid: Option<&str>, value_type: Option<&str>) -> Option<IcsTime> {
    let v = value.trim();
    if value_type.map(|t| t.eq_ignore_ascii_case("DATE")).unwrap_or(false) || (v.len() == 8 && !v.contains('T')) {
        let d = NaiveDate::parse_from_str(&v[..8], "%Y%m%d").ok()?;
        let naive = d.and_hms_opt(0, 0, 0)?;
        let local = chrono::Local.from_local_datetime(&naive).earliest()?;
        return Some(IcsTime {
            utc: local.with_timezone(&Utc),
            all_day: true,
            naive,
            tz: None,
            is_utc: false,
        });
    }
    let is_utc = v.ends_with('Z');
    let naive = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    if is_utc {
        return Some(IcsTime {
            utc: Utc.from_utc_datetime(&naive),
            all_day: false,
            naive,
            tz: None,
            is_utc: true,
        });
    }
    let tz = tzid.and_then(parse_tz);
    let utc = match tz {
        Some(tz) => tz.from_local_datetime(&naive).earliest()?.with_timezone(&Utc),
        None => chrono::Local
            .from_local_datetime(&naive)
            .earliest()?
            .with_timezone(&Utc),
    };
    Some(IcsTime {
        utc,
        all_day: false,
        naive,
        tz,
        is_utc: false,
    })
}

/// ISO-8601 duration as used by iCalendar (e.g. PT1H30M, P1D).
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let mut total = Duration::zero();
    let mut num = String::new();
    for ch in s.chars() {
        match ch {
            '0'..='9' => num.push(ch),
            'T' => {}
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match ch {
                    'W' => Duration::weeks(n),
                    'D' => Duration::days(n),
                    'H' => Duration::hours(n),
                    'M' => Duration::minutes(n),
                    _ => Duration::seconds(n),
                };
            }
            _ => return None,
        }
    }
    Some(if neg { -total } else { total })
}

/// Unescape iCalendar TEXT values.
fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Find a meeting link (Zoom, Meet, Teams, Webex, ...) in free text.
pub fn find_join_url(texts: &[&str]) -> Option<String> {
    const HOSTS: &[&str] = &[
        "zoom.us/j",
        "zoom.us/my",
        "zoom.us/w",
        "meet.google.com/",
        "teams.microsoft.com/l/meetup-join",
        "teams.live.com/meet",
        "webex.com/",
        "whereby.com/",
        "meet.jit.si/",
        "gotomeet",
        "chime.aws/",
        "bluejeans.com/",
        "discord.gg/",
        "slack.com/huddle",
    ];
    for text in texts {
        for (i, _) in text.match_indices("http") {
            let rest = &text[i..];
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | ')' | ']' | '\\'))
                .unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ';']);
            if HOSTS.iter().any(|h| url.contains(h)) {
                return Some(url.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Parsing + expansion
// ---------------------------------------------------------------------------

struct RawEvent {
    uid: String,
    title: String,
    location: String,
    description: String,
    url: String,
    start: IcsTime,
    end: Option<IcsTime>,
    duration: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<i64>,
    recurrence_id: Option<i64>,
    cancelled: bool,
}

fn rrule_dtstart(t: &IcsTime) -> String {
    if t.is_utc {
        format!("DTSTART:{}Z", t.naive.format("%Y%m%dT%H%M%S"))
    } else if let Some(tz) = t.tz {
        format!("DTSTART;TZID={}:{}", tz.name(), t.naive.format("%Y%m%dT%H%M%S"))
    } else {
        format!("DTSTART:{}", t.naive.format("%Y%m%dT%H%M%S"))
    }
}

fn occurrences(ev: &RawEvent, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<i64> {
    let Some(rule) = &ev.rrule else {
        return vec![ev.start.utc.timestamp_millis()];
    };
    let text = format!("{}\nRRULE:{}", rrule_dtstart(&ev.start), rule);
    let set: rrule::RRuleSet = match text.parse() {
        Ok(s) => s,
        // Unsupported rule: show the first instance only.
        Err(_) => return vec![ev.start.utc.timestamp_millis()],
    };
    let set = set
        .after(from.with_timezone(&rrule::Tz::UTC))
        .before(to.with_timezone(&rrule::Tz::UTC));
    set.all(500)
        .dates
        .into_iter()
        .map(|d| d.with_timezone(&Utc).timestamp_millis())
        .filter(|ms| !ev.exdates.contains(ms))
        .collect()
}

pub fn parse_ics(text: &str, calendar_id: i64, color: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<CalEvent> {
    let mut raws: Vec<RawEvent> = Vec::new();
    for cal in ical::IcalParser::new(std::io::BufReader::new(text.as_bytes())).flatten() {
        for e in cal.events {
            let mut r = RawEvent {
                uid: String::new(),
                title: String::new(),
                location: String::new(),
                description: String::new(),
                url: String::new(),
                start: match parse_time("19700101T000000Z", None, None) {
                    Some(t) => t,
                    None => continue,
                },
                end: None,
                duration: None,
                rrule: None,
                exdates: Vec::new(),
                recurrence_id: None,
                cancelled: false,
            };
            let mut has_start = false;
            for p in &e.properties {
                let v = p.value.clone().unwrap_or_default();
                match p.name.to_uppercase().as_str() {
                    "UID" => r.uid = v,
                    "SUMMARY" => r.title = unescape(&v),
                    "LOCATION" => r.location = unescape(&v),
                    "DESCRIPTION" => r.description = unescape(&v),
                    "URL" => r.url = v,
                    "STATUS" => r.cancelled = v.eq_ignore_ascii_case("CANCELLED"),
                    "DTSTART" => {
                        if let Some(t) = parse_time(&v, param(p, "TZID"), param(p, "VALUE")) {
                            r.start = t;
                            has_start = true;
                        }
                    }
                    "DTEND" => r.end = parse_time(&v, param(p, "TZID"), param(p, "VALUE")),
                    "DURATION" => r.duration = parse_duration(&v),
                    "RRULE" => r.rrule = Some(v),
                    "EXDATE" => {
                        for part in v.split(',') {
                            if let Some(t) = parse_time(part, param(p, "TZID"), param(p, "VALUE")) {
                                r.exdates.push(t.utc.timestamp_millis());
                            }
                        }
                    }
                    "RECURRENCE-ID" => {
                        r.recurrence_id =
                            parse_time(&v, param(p, "TZID"), param(p, "VALUE")).map(|t| t.utc.timestamp_millis())
                    }
                    _ => {}
                }
            }
            if has_start {
                raws.push(r);
            }
        }
    }
    // Instances overridden by a RECURRENCE-ID event are replaced by that event.
    let overridden: HashSet<(String, i64)> = raws
        .iter()
        .filter_map(|r| r.recurrence_id.map(|rid| (r.uid.clone(), rid)))
        .collect();
    let mut out = Vec::new();
    for r in &raws {
        if r.cancelled {
            continue;
        }
        let length = match (&r.end, r.duration) {
            (Some(end), _) => end.utc - r.start.utc,
            (None, Some(d)) => d,
            (None, None) if r.start.all_day => Duration::days(1),
            _ => Duration::zero(),
        };
        let length = length.max(Duration::zero());
        let join = find_join_url(&[&r.url, &r.location, &r.description]);
        let window_from = from - length;
        for start in occurrences(r, window_from, to) {
            if r.recurrence_id.is_none() && overridden.contains(&(r.uid.clone(), start)) {
                continue;
            }
            let end = start + length.num_milliseconds();
            if end < from.timestamp_millis() || start > to.timestamp_millis() {
                continue;
            }
            out.push(CalEvent {
                id: format!("{}:{}:{}", calendar_id, r.uid, start),
                calendar_id,
                title: if r.title.is_empty() {
                    "(No title)".into()
                } else {
                    r.title.clone()
                },
                start,
                end,
                all_day: r.start.all_day,
                location: r.location.clone(),
                join_url: join.clone(),
                color: color.to_string(),
            });
        }
    }
    out.sort_by_key(|e| e.start);
    out
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

fn calendars() -> Vec<Calendar> {
    db().prepare("SELECT id, name, source, color, enabled, last_fetch, last_error FROM calendars ORDER BY id")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(Calendar {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    source: r.get(2)?,
                    color: r.get(3)?,
                    enabled: r.get::<_, i64>(4)? != 0,
                    last_fetch: r.get(5)?,
                    last_error: r.get(6)?,
                })
            })
            .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default()
}

async fn fetch(source: &str) -> Result<String, String> {
    let s = source.trim();
    let url = if let Some(rest) = s.strip_prefix("webcal://") {
        format!("https://{rest}")
    } else {
        s.to_string()
    };
    if url.starts_with("http://") || url.starts_with("https://") {
        let resp = http().get(&url).send().await.map_err(err)?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.text().await.map_err(err)
    } else {
        std::fs::read_to_string(s.trim_matches('"')).map_err(err)
    }
}

/// Re-expand every calendar from its cached ICS text.
fn rebuild() {
    let rows: Vec<(i64, String, Option<String>)> = db()
        .prepare("SELECT id, color, cache FROM calendars WHERE enabled = 1")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map(|r| r.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    let now = Utc::now();
    let from = now - Duration::days(1);
    let to = now + Duration::days(14);
    let mut all = Vec::new();
    for (id, color, cache) in rows {
        if let Some(text) = cache {
            all.extend(parse_ics(&text, id, &color, from, to));
        }
    }
    all.sort_by_key(|e| e.start);
    *EVENTS.lock() = all;
    notify::emit("calendar", json!({}));
}

pub async fn refresh_all() {
    for c in calendars().into_iter().filter(|c| c.enabled) {
        let result = fetch(&c.source).await;
        let conn = db();
        let _ = match result {
            Ok(text) if text.contains("BEGIN:VCALENDAR") => conn.execute(
                "UPDATE calendars SET cache = ?1, last_fetch = ?2, last_error = NULL WHERE id = ?3",
                params![text, now_ms(), c.id],
            ),
            Ok(_) => conn.execute(
                "UPDATE calendars SET last_error = 'Not an iCalendar file', last_fetch = ?1 WHERE id = ?2",
                params![now_ms(), c.id],
            ),
            Err(e) => conn.execute(
                "UPDATE calendars SET last_error = ?1, last_fetch = ?2 WHERE id = ?3",
                params![e, now_ms(), c.id],
            ),
        };
    }
    rebuild();
}

pub fn start() {
    rebuild();
    crate::util::spawn_loop("calendar", || {
        loop {
            tauri::async_runtime::block_on(refresh_all());
            let mins = settings::get_i64("calendar.refreshMin", 15).clamp(5, 1440);
            sleep_ms(mins as u64 * 60_000);
        }
    });
}

pub fn alerts_tick() {
    let lead = settings::get_i64("calendar.alertMin", 10).max(0) * 60_000;
    if lead == 0 {
        return;
    }
    let now = now_ms();
    let due: Vec<CalEvent> = EVENTS
        .lock()
        .iter()
        .filter(|e| !e.all_day && e.start > now && e.start - lead <= now)
        .cloned()
        .collect();
    for e in due {
        if ALERTED.lock().insert(e.id.clone()) {
            let mins = ((e.start - now) as f64 / 60_000.0).ceil() as i64;
            notify::alert(
                "event",
                &e.title,
                &format!(
                    "Starts in {mins} min{}",
                    if e.location.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", e.location)
                    }
                ),
                json!(e),
            );
        }
    }
}

#[tauri::command]
pub fn calendar_events() -> Vec<CalEvent> {
    EVENTS.lock().clone()
}

#[tauri::command]
pub fn calendar_list() -> Vec<Calendar> {
    calendars()
}

#[tauri::command]
pub async fn calendar_add(name: String, source: String, color: Option<String>) -> CmdResult<()> {
    let source = source.trim().to_string();
    if source.is_empty() {
        return Err("Enter an iCal URL or .ics file path".into());
    }
    let text = fetch(&source).await?;
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("That address did not return an iCalendar (.ics) file".into());
    }
    let name = if name.trim().is_empty() {
        "Calendar".to_string()
    } else {
        name.trim().to_string()
    };
    db().execute(
        "INSERT INTO calendars(name, source, color, cache, last_fetch) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, source, color.unwrap_or_else(|| "#60a5fa".into()), text, now_ms()],
    )
    .map_err(err)?;
    rebuild();
    Ok(())
}

#[tauri::command]
pub fn calendar_remove(id: i64) -> CmdResult<()> {
    db().execute("DELETE FROM calendars WHERE id = ?1", [id]).map_err(err)?;
    rebuild();
    Ok(())
}

#[tauri::command]
pub fn calendar_toggle(id: i64, enabled: bool) -> CmdResult<()> {
    db().execute(
        "UPDATE calendars SET enabled = ?1 WHERE id = ?2",
        params![enabled as i64, id],
    )
    .map_err(err)?;
    rebuild();
    Ok(())
}

#[tauri::command]
pub async fn calendar_refresh() -> CmdResult<()> {
    refresh_all().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a1\r\nSUMMARY:Standup\r\nDTSTART;TZID=America/New_York:20261026T090000\r\nDTEND;TZID=America/New_York:20261026T091500\r\nRRULE:FREQ=DAILY;COUNT=5\r\nEXDATE;TZID=America/New_York:20261027T090000\r\nDESCRIPTION:Join: https://meet.google.com/abc-defg-hij\\nThanks\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b2\r\nSUMMARY:Lunch\\, team\r\nDTSTART:20261028T170000Z\r\nDURATION:PT1H\r\nLOCATION:Cafe\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:c3\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261029\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn parses_and_expands() {
        let from = Utc.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
        let evs = parse_ics(ICS, 1, "#fff", from, to);
        let standups: Vec<_> = evs.iter().filter(|e| e.title == "Standup").collect();
        // 5 occurrences minus one EXDATE.
        assert_eq!(standups.len(), 4);
        // 09:00 New York (EDT, UTC-4) = 13:00 UTC on Oct 26.
        assert_eq!(
            standups[0].start,
            Utc.with_ymd_and_hms(2026, 10, 26, 13, 0, 0).unwrap().timestamp_millis()
        );
        assert_eq!(standups[0].end - standups[0].start, 15 * 60_000);
        assert_eq!(
            standups[0].join_url.as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        let lunch = evs.iter().find(|e| e.title == "Lunch, team").unwrap();
        assert_eq!(lunch.end - lunch.start, 3_600_000);
        assert_eq!(lunch.location, "Cafe");
        let holiday = evs.iter().find(|e| e.title == "Holiday").unwrap();
        assert!(holiday.all_day);
    }

    #[test]
    fn durations_and_tz() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1D"), Some(Duration::days(1)));
        assert_eq!(
            parse_tz("Eastern Standard Time").map(|t| t.name()),
            Some("America/New_York")
        );
        assert_eq!(
            parse_tz("/mozilla.org/20050126_1/America/New_York").map(|t| t.name()),
            Some("America/New_York")
        );
    }

    #[test]
    fn join_links() {
        assert_eq!(
            find_join_url(&["", "Room 4", "Click https://us02web.zoom.us/j/123?pwd=x. Thanks"]),
            Some("https://us02web.zoom.us/j/123?pwd=x".into())
        );
        assert_eq!(find_join_url(&["https://example.com"]), None);
    }
}
