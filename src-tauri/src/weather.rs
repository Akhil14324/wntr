//! Weather from Open-Meteo (no key). The city is geocoded once; no location tracking.

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::AppHandle;

use crate::settings;
use crate::util::{CmdResult, err, http, now_ms};

static CACHE: Mutex<Option<(i64, String, Value)>> = Mutex::new(None);
const TTL_MS: i64 = 30 * 60 * 1000;

#[derive(Deserialize)]
struct GeoResult {
    results: Option<Vec<GeoPlace>>,
}

#[derive(Deserialize, Clone)]
struct GeoPlace {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
    admin1: Option<String>,
}

async fn geocode(city: &str) -> CmdResult<GeoPlace> {
    let r: GeoResult = http()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", city), ("count", "1"), ("language", "en"), ("format", "json")])
        .send()
        .await
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;
    r.results
        .and_then(|v| v.into_iter().next())
        .ok_or_else(|| format!("Couldn't find \"{city}\""))
}

/// Set the city: geocode it and store coordinates + display name.
#[tauri::command]
pub async fn weather_set_city(app: AppHandle, city: String) -> CmdResult<Value> {
    let city = city.trim().to_string();
    if city.is_empty() {
        settings::patch(
            &app,
            &json!({ "weather": { "city": "", "lat": null, "lon": null, "place": "" } }),
        );
        *CACHE.lock() = None;
        return Ok(Value::Null);
    }
    let p = geocode(&city).await?;
    let place = [Some(p.name.clone()), p.admin1.clone(), p.country.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    settings::patch(
        &app,
        &json!({ "weather": { "city": city, "lat": p.latitude, "lon": p.longitude, "place": place } }),
    );
    *CACHE.lock() = None;
    weather_get(false).await
}

#[tauri::command]
pub async fn weather_get(force: bool) -> CmdResult<Value> {
    let lat = settings::get("weather.lat").as_f64();
    let lon = settings::get("weather.lon").as_f64();
    let (Some(lat), Some(lon)) = (lat, lon) else {
        return Ok(Value::Null);
    };
    let unit = if settings::get_str("weather.unit") == "fahrenheit" {
        "fahrenheit"
    } else {
        "celsius"
    };
    let key = format!("{lat:.3},{lon:.3},{unit}");
    if !force
        && let Some((at, k, v)) = CACHE.lock().clone()
        && k == key
        && now_ms() - at < TTL_MS
    {
        return Ok(v);
    }
    let v: Value = http()
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,is_day,wind_speed_10m".into(),
            ),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max".into(),
            ),
            ("timezone", "auto".into()),
            ("forecast_days", "4".into()),
            ("temperature_unit", unit.into()),
        ])
        .send()
        .await
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;
    let out = json!({
        "place": settings::get_str("weather.place"),
        "unit": unit,
        "current": v.get("current"),
        "daily": v.get("daily"),
        "fetchedAt": now_ms(),
    });
    *CACHE.lock() = Some((now_ms(), key, out.clone()));
    Ok(out)
}
