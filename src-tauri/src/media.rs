//! Now playing via Windows System Media Transport Controls (SMTC). Any app that
//! publishes media info shows up: Spotify, Apple Music, Media Player and
//! browsers (YouTube Music, ...).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use parking_lot::Mutex;
use serde::Serialize;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{DataReader, IInputStream};
use windows_core::Interface;

use crate::util::{CmdResult, com_mta, err, now_ms, sleep_ms, sub_dir};
use crate::{notify, settings};

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    id: String,
    app: String,
    playing: bool,
    title: String,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    id: String,
    app: String,
    title: String,
    artist: String,
    album: String,
    art: Option<String>,
    duration_ms: i64,
    position_ms: i64,
    position_at: i64,
    playing: bool,
    can_next: bool,
    can_prev: bool,
    can_seek: bool,
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct MediaPayload {
    sessions: Vec<SessionInfo>,
    current: Option<NowPlaying>,
}

static SELECTED: Mutex<Option<String>> = Mutex::new(None);
static CURRENT_ID: Mutex<Option<String>> = Mutex::new(None);
static LAST: Mutex<Option<MediaPayload>> = Mutex::new(None);
static REFRESH: AtomicBool = AtomicBool::new(false);

/// Friendly app name from an AppUserModelID.
pub fn app_name(aumid: &str) -> String {
    let l = aumid.to_lowercase();
    let known = [
        ("spotify", "Spotify"),
        ("chrome", "Chrome"),
        ("msedge", "Edge"),
        ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"),
        ("zunemusic", "Media Player"),
        ("applemusic", "Apple Music"),
        ("itunes", "iTunes"),
        ("youtube music", "YouTube Music"),
        ("youtube-music", "YouTube Music"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vlc", "VLC"),
        ("foobar", "foobar2000"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("amazon", "Amazon Music"),
    ];
    for (k, v) in known {
        if l.contains(k) {
            return v.to_string();
        }
    }
    // Store apps: "Publisher.AppName_hash!App" → "AppName".
    if let Some((package, _)) = aumid.split_once('!') {
        let family = package.split('_').next().unwrap_or(package);
        let name = family.rsplit('.').next().unwrap_or(family);
        if !name.is_empty() {
            return name.to_string();
        }
    }
    let base = aumid.rsplit(['\\', '/']).next().unwrap_or(aumid);
    base.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
}

fn source_key(app: &str) -> &'static str {
    match app {
        "Chrome" => "chrome",
        "Edge" => "edge",
        "Firefox" => "firefox",
        "Spotify" => "spotify",
        _ => "other",
    }
}

fn allowed(app: &str) -> bool {
    settings::get_bool(&format!("music.sources.{}", source_key(app)), true)
}

fn manager() -> windows::core::Result<Manager> {
    Manager::RequestAsync()?.join()
}

fn sessions(mgr: &Manager) -> Vec<Session> {
    let mut out = Vec::new();
    if let Ok(list) = mgr.GetSessions() {
        let n = list.Size().unwrap_or(0);
        for i in 0..n {
            if let Ok(s) = list.GetAt(i) {
                out.push(s);
            }
        }
    }
    out
}

fn is_playing(s: &Session) -> bool {
    s.GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|st| st == Status::Playing)
        .unwrap_or(false)
}

fn ticks_to_ms(t: i64) -> i64 {
    t / 10_000
}

/// WinRT DateTime (100ns since 1601) to unix ms.
fn datetime_to_unix_ms(ut: i64) -> i64 {
    (ut - 116_444_736_000_000_000) / 10_000
}

fn read_thumbnail(props: &Props) -> Option<Vec<u8>> {
    let r = props.Thumbnail().ok()?;
    let stream = r.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 10 * 1024 * 1024 {
        return None;
    }
    let input: IInputStream = stream.cast().ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

fn mime_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/png"
    }
}

/// Album art as a data URL, cached on disk by track.
fn art_for(track_key: &str, props: &Props) -> Option<String> {
    let file = sub_dir("art").join(crate::util::hash_bytes(track_key.as_bytes()));
    let bytes = match std::fs::read(&file) {
        Ok(b) if !b.is_empty() => b,
        _ => {
            let b = read_thumbnail(props)?;
            let _ = std::fs::write(&file, &b);
            b
        }
    };
    Some(format!(
        "data:{};base64,{}",
        mime_of(&bytes),
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

struct Cached {
    title: String,
    artist: String,
    album: String,
    art: Option<String>,
    fetched_at: i64,
    art_tries: u32,
}

/// Pick the session to show: the user's pinned choice, else a playing session
/// (preferring Windows' current one), else the current session.
fn pick<'a>(mgr: &Manager, list: &'a [(Session, SessionInfo)]) -> Option<&'a (Session, SessionInfo)> {
    if let Some(sel) = SELECTED.lock().clone()
        && let Some(found) = list.iter().find(|(_, i)| i.id == sel)
    {
        return Some(found);
    }
    let current_id = mgr
        .GetCurrentSession()
        .and_then(|s| s.SourceAppUserModelId())
        .map(|h| h.to_string())
        .ok();
    let current = current_id
        .as_ref()
        .and_then(|id| list.iter().find(|(_, i)| &i.id == id));
    if let Some(c) = current
        && c.1.playing
    {
        return Some(c);
    }
    list.iter().find(|(_, i)| i.playing).or(current).or(list.first())
}

fn poll(mgr: &Manager, cache: &mut HashMap<String, Cached>) -> MediaPayload {
    let now = now_ms();
    let mut list: Vec<(Session, SessionInfo)> = Vec::new();
    for s in sessions(mgr) {
        let id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        let app = app_name(&id);
        if !allowed(&app) {
            continue;
        }
        let playing = is_playing(&s);
        let title = cache.get(&id).map(|c| c.title.clone()).unwrap_or_default();
        list.push((
            s,
            SessionInfo {
                id,
                app,
                playing,
                title,
            },
        ));
    }
    let picked = pick(mgr, &list);
    let mut current = None;
    if let Some((s, info)) = picked {
        let force = REFRESH.swap(false, Ordering::Relaxed);
        let stale = cache
            .get(&info.id)
            .map(|c| force || now - c.fetched_at > 1200)
            .unwrap_or(true);
        if stale && let Ok(props) = s.TryGetMediaPropertiesAsync().and_then(|op| op.join()) {
            let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
            let mut artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
            if artist.is_empty() {
                artist = props.AlbumArtist().map(|h| h.to_string()).unwrap_or_default();
            }
            let album = props.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
            let key = format!("{}|{}|{}", title, artist, album);
            let prev = cache.get(&info.id);
            let same_track = prev
                .map(|c| format!("{}|{}|{}", c.title, c.artist, c.album) == key)
                .unwrap_or(false);
            let (mut art, mut tries) = if same_track {
                (prev.and_then(|c| c.art.clone()), prev.map(|c| c.art_tries).unwrap_or(0))
            } else {
                (None, 0)
            };
            // Browsers often publish artwork a moment after the title.
            if art.is_none() && tries < 8 && settings::get_bool("music.showArt", true) {
                art = art_for(&key, &props);
                tries += 1;
            }
            cache.insert(
                info.id.clone(),
                Cached {
                    title,
                    artist,
                    album,
                    art,
                    fetched_at: now,
                    art_tries: tries,
                },
            );
        }
        if let Some(c) = cache.get(&info.id) {
            let pinfo = s.GetPlaybackInfo().ok();
            let controls = pinfo.as_ref().and_then(|p| p.Controls().ok());
            let tl = s.GetTimelineProperties().ok();
            let (mut duration, mut position, mut at) = (0, 0, now);
            if let Some(tl) = tl {
                let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
                let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
                duration = ticks_to_ms(end - start).max(0);
                position = ticks_to_ms(tl.Position().map(|t| t.Duration).unwrap_or(0) - start).max(0);
                let updated = tl
                    .LastUpdatedTime()
                    .map(|d| datetime_to_unix_ms(d.UniversalTime))
                    .unwrap_or(0);
                if updated > 0 && updated <= now {
                    at = updated;
                }
            }
            if !info.playing {
                at = now;
            }
            current = Some(NowPlaying {
                id: info.id.clone(),
                app: info.app.clone(),
                title: c.title.clone(),
                artist: c.artist.clone(),
                album: c.album.clone(),
                art: c.art.clone(),
                duration_ms: duration,
                position_ms: position,
                position_at: at,
                playing: info.playing,
                can_next: controls.as_ref().and_then(|c| c.IsNextEnabled().ok()).unwrap_or(true),
                can_prev: controls
                    .as_ref()
                    .and_then(|c| c.IsPreviousEnabled().ok())
                    .unwrap_or(true),
                can_seek: controls
                    .as_ref()
                    .and_then(|c| c.IsPlaybackPositionEnabled().ok())
                    .unwrap_or(false),
            });
        }
    }
    *CURRENT_ID.lock() = current.as_ref().map(|c| c.id.clone());
    let sessions = list
        .into_iter()
        .map(|(_, mut i)| {
            if let Some(c) = cache.get(&i.id) {
                i.title = c.title.clone();
            }
            i
        })
        .collect();
    MediaPayload { sessions, current }
}

fn projected(np: &NowPlaying, now: i64) -> i64 {
    if np.playing {
        np.position_ms + (now - np.position_at)
    } else {
        np.position_ms
    }
}

/// Only emit when something visible changed or the position jumped (seek).
fn should_emit(prev: &Option<MediaPayload>, next: &MediaPayload, now: i64) -> bool {
    let Some(prev) = prev else { return true };
    if prev.sessions != next.sessions {
        return true;
    }
    match (&prev.current, &next.current) {
        (None, None) => false,
        (Some(a), Some(b)) => {
            a.id != b.id
                || a.title != b.title
                || a.artist != b.artist
                || a.album != b.album
                || a.art != b.art
                || a.playing != b.playing
                || a.duration_ms != b.duration_ms
                || a.can_seek != b.can_seek
                || (projected(a, now) - projected(b, now)).abs() > 1500
        }
        _ => true,
    }
}

pub fn start() {
    crate::util::spawn_loop("media", || {
        com_mta();
        let mgr = loop {
            match manager() {
                Ok(m) => break m,
                Err(_) => sleep_ms(5000),
            }
        };
        let mut cache: HashMap<String, Cached> = HashMap::new();
        let mut last_emit = 0i64;
        loop {
            let payload = poll(&mgr, &mut cache);
            let now = now_ms();
            let prev = LAST.lock().clone();
            if should_emit(&prev, &payload, now) || now - last_emit > 15_000 {
                notify::emit("media", &payload);
                *LAST.lock() = Some(payload);
                last_emit = now;
            }
            // Faster polling right after a command so the UI reacts quickly.
            for _ in 0..8 {
                if REFRESH.load(Ordering::Relaxed) {
                    break;
                }
                sleep_ms(100);
            }
        }
    });
}

fn with_session<F: FnOnce(&Session) -> windows::core::Result<()>>(f: F) -> CmdResult<()> {
    com_mta();
    let mgr = manager().map_err(err)?;
    let id = CURRENT_ID.lock().clone().ok_or("Nothing is playing")?;
    let s = sessions(&mgr)
        .into_iter()
        .find(|s| s.SourceAppUserModelId().map(|h| h == id).unwrap_or(false))
        .ok_or("Media session ended")?;
    f(&s).map_err(err)?;
    REFRESH.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn media_control(action: String, position_ms: Option<i64>) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        with_session(|s| match action.as_str() {
            "toggle" => s.TryTogglePlayPauseAsync()?.join().map(|_| ()),
            "play" => s.TryPlayAsync()?.join().map(|_| ()),
            "pause" => s.TryPauseAsync()?.join().map(|_| ()),
            "next" => s.TrySkipNextAsync()?.join().map(|_| ()),
            "prev" => s.TrySkipPreviousAsync()?.join().map(|_| ()),
            "seek" => {
                let start = s.GetTimelineProperties()?.StartTime()?.Duration;
                let ticks = start + position_ms.unwrap_or(0).max(0) * 10_000;
                s.TryChangePlaybackPositionAsync(ticks)?.join().map(|_| ())
            }
            _ => Ok(()),
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn media_select(id: Option<String>) {
    *SELECTED.lock() = id;
    REFRESH.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn media_state() -> MediaPayload {
    LAST.lock().clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("Chrome"), "Chrome");
        assert_eq!(app_name("MSEdge"), "Edge");
        assert_eq!(
            app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "Media Player"
        );
        assert_eq!(app_name("C:\\Apps\\foo.exe"), "foo");
        assert_eq!(app_name("52971PietroDisc.AppMusic_eft9pwkrwnhsy!App"), "AppMusic");
    }

    #[test]
    fn datetime_conversion() {
        // 2020-01-01T00:00:00Z
        assert_eq!(datetime_to_unix_ms(132_223_104_000_000_000), 1_577_836_800_000);
    }
}
