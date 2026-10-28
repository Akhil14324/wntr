//! Synced lyrics from LRCLIB (no key, no account), cached in SQLite.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::db;
use crate::util::{CmdResult, err, http, now_ms};

const API: &str = "https://lrclib.net/api";
/// How long a "not found" result is trusted before we ask LRCLIB again.
const MISS_TTL_MS: i64 = 3 * 24 * 3600 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub track: String,
    pub artist: String,
    pub source: String,
    pub key: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct LrcRecord {
    id: i64,
    track_name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    id: i64,
    track: String,
    artist: String,
    album: String,
    duration: f64,
    synced: bool,
}

/// Strip video/upload noise so the lookup matches the real song title.
pub fn clean_title(title: &str) -> String {
    let noise = [
        "official video",
        "official music video",
        "official audio",
        "official lyric video",
        "lyric video",
        "lyrics",
        "lyric",
        "audio",
        "official",
        "music video",
        "mv",
        "m/v",
        "visualizer",
        "official visualizer",
        "hd",
        "hq",
        "4k",
        "remastered",
        "remaster",
        "explicit",
        "clean",
        "video",
        "performance video",
        "dance practice",
    ];
    let mut out = String::new();
    let mut depth = 0;
    let mut group = String::new();
    for ch in title.chars() {
        match ch {
            '(' | '[' | '【' => {
                depth += 1;
                group.clear();
            }
            ')' | ']' | '】' if depth > 0 => {
                depth -= 1;
                let g = group.trim().to_lowercase();
                let is_noise = noise
                    .iter()
                    .any(|n| g == *n || g.starts_with(&format!("{n} ")) || g.ends_with(&format!(" {n}")))
                    || g.contains("official")
                    || g.contains("remaster")
                    || g.contains("lyric");
                // Keep meaningful groups such as (feat. X) or (Remix).
                if !is_noise && !g.starts_with("feat") && !g.starts_with("ft") {
                    out.push_str(&format!("({}) ", group.trim()));
                }
                group.clear();
            }
            _ if depth > 0 => group.push(ch),
            _ => out.push(ch),
        }
    }
    let mut s = out.split_whitespace().collect::<Vec<_>>().join(" ");
    for sep in [" | ", " // "] {
        if let Some(i) = s.find(sep) {
            s.truncate(i);
        }
    }
    for suffix in [" - Topic", " (Official)", " feat.", " ft."] {
        if let Some(i) = s.find(suffix) {
            s.truncate(i);
        }
    }
    s.trim().trim_end_matches(['-', '|', ',']).trim().to_string()
}

pub fn clean_artist(artist: &str) -> String {
    let a = artist.trim();
    let a = a.strip_suffix(" - Topic").unwrap_or(a);
    let a = a.strip_suffix("VEVO").unwrap_or(a);
    a.trim().to_string()
}

/// Alternative artist strings: first credited artist, before "feat.", etc.
fn artist_variants(artist: &str) -> Vec<String> {
    let mut v = vec![artist.to_string()];
    let lower = artist.to_lowercase();
    for sep in [
        " feat. ", " feat ", " ft. ", " ft ", " & ", " x ", ", ", " and ", " with ", " • ", "、",
    ] {
        if let Some(i) = lower.find(sep) {
            let first = artist[..i].trim().to_string();
            if !first.is_empty() && !v.contains(&first) {
                v.push(first);
            }
        }
    }
    v
}

pub fn cache_key(title: &str, artist: &str) -> String {
    format!(
        "{}|{}",
        clean_title(title).to_lowercase(),
        clean_artist(artist).to_lowercase()
    )
}

fn from_cache(key: &str) -> Option<(Lyrics, i64)> {
    db().query_row(
        "SELECT synced, plain, track, artist, fetched_at FROM lyrics_cache WHERE key = ?1",
        [key],
        |r| {
            Ok((
                Lyrics {
                    synced: r.get(0)?,
                    plain: r.get(1)?,
                    track: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    artist: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    source: "cache".into(),
                    key: key.to_string(),
                },
                r.get(4)?,
            ))
        },
    )
    .ok()
}

fn store(key: &str, l: &Lyrics) {
    let _ = db().execute(
        "INSERT OR REPLACE INTO lyrics_cache(key, synced, plain, track, artist, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![key, l.synced, l.plain, l.track, l.artist, now_ms()],
    );
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|t| !t.trim().is_empty()).cloned()
}

fn to_lyrics(r: &LrcRecord, key: &str) -> Lyrics {
    let instrumental = r.instrumental.unwrap_or(false);
    Lyrics {
        synced: non_empty(&r.synced_lyrics),
        plain: if instrumental {
            Some("♪ Instrumental ♪".into())
        } else {
            non_empty(&r.plain_lyrics)
        },
        track: r.track_name.clone().unwrap_or_default(),
        artist: r.artist_name.clone().unwrap_or_default(),
        source: "lrclib".into(),
        key: key.to_string(),
    }
}

async fn get_exact(title: &str, artist: &str, album: &str, duration_s: f64) -> Option<LrcRecord> {
    let mut q: Vec<(&str, String)> = vec![("track_name", title.into()), ("artist_name", artist.into())];
    if !album.is_empty() {
        q.push(("album_name", album.into()));
    }
    if duration_s > 0.0 {
        q.push(("duration", format!("{}", duration_s.round() as i64)));
    }
    let resp = http().get(format!("{API}/get")).query(&q).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<LrcRecord>().await.ok()
}

async fn search(params: &[(&str, String)]) -> Vec<LrcRecord> {
    match http().get(format!("{API}/search")).query(params).send().await {
        Ok(r) if r.status().is_success() => r.json::<Vec<LrcRecord>>().await.unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Best search hit: synced lyrics first, then closest duration.
fn best(mut hits: Vec<LrcRecord>, duration_s: f64) -> Option<LrcRecord> {
    hits.retain(|h| non_empty(&h.synced_lyrics).is_some() || non_empty(&h.plain_lyrics).is_some());
    hits.sort_by(|a, b| {
        let sa = non_empty(&a.synced_lyrics).is_some();
        let sb = non_empty(&b.synced_lyrics).is_some();
        let da = (a.duration.unwrap_or(0.0) - duration_s).abs();
        let db = (b.duration.unwrap_or(0.0) - duration_s).abs();
        sb.cmp(&sa)
            .then(da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal))
    });
    if duration_s > 0.0 {
        // Reject hits whose length is way off (different song with the same name).
        hits.retain(|h| h.duration.map(|d| (d - duration_s).abs() < 20.0).unwrap_or(true));
    }
    hits.into_iter().next()
}

async fn lookup(title: &str, artist: &str, album: &str, duration_s: f64) -> Option<LrcRecord> {
    let t = clean_title(title);
    let a = clean_artist(artist);
    // Titles like "Artist - Song" from video uploads.
    let (t, a) = if a.is_empty() || t.contains(" - ") {
        match t.split_once(" - ") {
            Some((left, right)) if a.is_empty() || left.eq_ignore_ascii_case(&a) => {
                (right.trim().to_string(), left.trim().to_string())
            }
            _ => (t, a),
        }
    } else {
        (t, a)
    };
    for artist_v in artist_variants(&a) {
        if let Some(r) = get_exact(&t, &artist_v, album, duration_s).await {
            return Some(r);
        }
        if !album.is_empty()
            && let Some(r) = get_exact(&t, &artist_v, "", duration_s).await
        {
            return Some(r);
        }
        let hits = search(&[("track_name", t.clone()), ("artist_name", artist_v.clone())]).await;
        if let Some(r) = best(hits, duration_s) {
            return Some(r);
        }
    }
    // Title alone.
    best(search(&[("q", t.clone())]).await, duration_s)
}

#[tauri::command]
pub async fn lyrics_get(
    title: String,
    artist: String,
    album: Option<String>,
    duration_ms: Option<i64>,
) -> CmdResult<Lyrics> {
    let key = cache_key(&title, &artist);
    if let Some((l, at)) = from_cache(&key) {
        let miss = l.synced.is_none() && l.plain.is_none();
        if !miss || now_ms() - at < MISS_TTL_MS {
            return Ok(l);
        }
    }
    let duration_s = duration_ms.unwrap_or(0) as f64 / 1000.0;
    let result = lookup(&title, &artist, album.as_deref().unwrap_or(""), duration_s).await;
    let lyrics = match result {
        Some(r) => to_lyrics(&r, &key),
        None => Lyrics {
            source: "lrclib".into(),
            key: key.clone(),
            ..Default::default()
        },
    };
    store(&key, &lyrics);
    Ok(lyrics)
}

#[tauri::command]
pub async fn lyrics_search(query: String) -> CmdResult<Vec<SearchHit>> {
    let hits = search(&[("q", query)]).await;
    Ok(hits
        .into_iter()
        .take(20)
        .map(|h| SearchHit {
            id: h.id,
            track: h.track_name.unwrap_or_default(),
            artist: h.artist_name.unwrap_or_default(),
            album: h.album_name.unwrap_or_default(),
            duration: h.duration.unwrap_or(0.0),
            synced: non_empty(&h.synced_lyrics).is_some(),
        })
        .collect())
}

/// Use a specific LRCLIB record for the song identified by `key`.
#[tauri::command]
pub async fn lyrics_choose(key: String, id: i64) -> CmdResult<Lyrics> {
    let r: LrcRecord = http()
        .get(format!("{API}/get/{id}"))
        .send()
        .await
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;
    let l = to_lyrics(&r, &key);
    store(&key, &l);
    Ok(l)
}

#[tauri::command]
pub fn lyrics_forget(key: String) -> CmdResult<()> {
    db().execute("DELETE FROM lyrics_cache WHERE key = ?1", [key])
        .map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("OMG (Official MV)"), "OMG");
        assert_eq!(clean_title("Hype Boy [Official Video]"), "Hype Boy");
        assert_eq!(clean_title("Song (Lyrics)"), "Song");
        assert_eq!(clean_title("Song (Audio)"), "Song");
        assert_eq!(clean_title("Song (Remix)"), "Song (Remix)");
        assert_eq!(clean_title("Song (feat. Someone)"), "Song");
        assert_eq!(clean_title("Song | Live at X"), "Song");
    }

    #[test]
    fn artist_variants_split() {
        let v = artist_variants("NewJeans & Someone");
        assert_eq!(v, vec!["NewJeans & Someone".to_string(), "NewJeans".to_string()]);
        assert_eq!(clean_artist("NewJeans - Topic"), "NewJeans");
    }
}
