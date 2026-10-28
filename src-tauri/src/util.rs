use std::path::PathBuf;

use chrono::{Local, TimeZone};

pub type CmdResult<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// `%APPDATA%\WNTR`, created on first use.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("WNTR");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn sub_dir(name: &str) -> PathBuf {
    let dir = data_dir().join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Local calendar day (`YYYY-MM-DD`) for a unix-ms timestamp.
pub fn local_day(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn today() -> String {
    local_day(now_ms())
}

/// Unix ms of local midnight `days_ago` days before today.
pub fn start_of_day_ms(days_ago: i64) -> i64 {
    let today = Local::now().date_naive() - chrono::Duration::days(days_ago);
    Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Initialise COM as MTA on the current thread (ignored if already initialised).
pub fn com_mta() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
    }
}

pub fn spawn_loop<F: FnOnce() + Send + 'static>(name: &str, f: F) {
    let _ = std::thread::Builder::new().name(name.into()).spawn(f);
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub fn http() -> &'static reqwest::Client {
    static CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
        reqwest::Client::builder()
            .user_agent(concat!(
                "WNTR/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/Akhil14324/wntr)"
            ))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("http client")
    });
    &CLIENT
}

/// Short stable hash used for cache keys and dedupe.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}
