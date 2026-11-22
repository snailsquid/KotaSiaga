//! Open-Meteo -> KondisiCuaca. One point (Alun-alun Bandung) for the whole service area,
//! current conditions plus the next 6 hourly values.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;

use super::http::{FetchError, Upstream};
use crate::risk::inputs::{ForecastSample, WeatherState};

pub const SERVICE: &str = "open_meteo";
const LAT: f64 = -6.9175;
const LON: f64 = 107.6191;

pub fn url(base: &str) -> String {
    format!(
        "{base}/v1/forecast?latitude={LAT}&longitude={LON}&current=temperature_2m,precipitation\
         &hourly=precipitation,temperature_2m&forecast_hours=7&timezone=GMT"
    )
}

pub async fn fetch(up: &Upstream, base: &str) -> Result<WeatherState, FetchError> {
    let v = up.get_json(SERVICE, &url(base), &[]).await?;
    parse(&v)
}

fn time(s: &str) -> Option<DateTime<Utc>> {
    // Open-Meteo with timezone=GMT returns local-less ISO times like "2026-11-22T06:30".
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|t| t.and_utc())
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

pub fn parse(v: &Value) -> Result<WeatherState, FetchError> {
    let bad = |m: &str| FetchError::Invalid(format!("open-meteo: {m}"));
    let cur = v.get("current").ok_or_else(|| bad("no current"))?;
    let observed_at = cur
        .get("time")
        .and_then(Value::as_str)
        .and_then(time)
        .ok_or_else(|| bad("current.time"))?;
    let temp = cur
        .get("temperature_2m")
        .and_then(num)
        .ok_or_else(|| bad("current.temperature_2m"))?;
    // `current.precipitation` is the sum over the preceding interval (usually 15 min).
    let interval_s = cur
        .get("interval")
        .and_then(Value::as_f64)
        .filter(|i| *i > 0.0)
        .unwrap_or(3600.0);
    let precip = cur
        .get("precipitation")
        .and_then(num)
        .unwrap_or(0.0)
        .max(0.0);
    let rain_mm_h = (precip * 3600.0 / interval_s).clamp(0.0, 60.0);

    let hourly = v.get("hourly").ok_or_else(|| bad("no hourly"))?;
    let times = hourly
        .get("time")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("hourly.time"))?;
    let rain = hourly
        .get("precipitation")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("hourly.precipitation"))?;
    let temps = hourly
        .get("temperature_2m")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("hourly.temperature_2m"))?;
    let forecast: Vec<ForecastSample> = times
        .iter()
        .zip(rain)
        .zip(temps)
        .filter_map(|((t, r), tc)| {
            Some(ForecastSample {
                at: time(t.as_str()?)?,
                rain_mm_h: num(r)?.max(0.0),
                temperature_c: num(tc)?,
            })
        })
        .filter(|p| p.at > observed_at)
        .take(6)
        .collect();

    let mut w = WeatherState {
        available: true,
        observed_at,
        base_rain_mm_h: rain_mm_h,
        storm: None,
        temperature_offset_c: 0.0,
        live_forecast: Some(forecast),
    };
    // Make the model's temperature for this rain equal the observed one.
    w.temperature_offset_c = temp - w.temperature(rain_mm_h);
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"latitude":-6.92,"longitude":107.6,"current":{"time":"2026-11-22T06:30","interval":900,"temperature_2m":28.3,"precipitation":0.5},"hourly":{"time":["2026-11-22T06:00","2026-11-22T07:00","2026-11-22T08:00","2026-11-22T09:00","2026-11-22T10:00","2026-11-22T11:00","2026-11-22T12:00"],"precipitation":[0.2,0.4,1.7,0.3,0.2,0.1,0.0],"temperature_2m":[28.4,28.2,25.7,25.6,25.2,24.6,24.1]}}"#;

    #[test]
    fn maps_current_and_forecast() {
        let w = parse(&serde_json::from_str(SAMPLE).unwrap()).unwrap();
        assert!(w.available);
        assert_eq!(w.base_rain_mm_h, 2.0, "0.5 mm per 15 min = 2 mm/h");
        let v = w.view_at(-6.9, 107.6);
        assert_eq!(v.temperature_c, 28.3);
        assert_eq!(v.rain_mm_h, 2.0);
        assert_eq!(v.forecast_6h.len(), 6);
        assert_eq!(
            v.forecast_6h[0].rain_mm_h, 0.4,
            "first hour after 06:30 is 07:00"
        );
        assert_eq!(v.forecast_6h[1].temperature_c, 25.7);
    }

    #[test]
    fn rejects_wrong_shape() {
        assert!(parse(&serde_json::json!({"error": true, "reason": "x"})).is_err());
        assert!(parse(&serde_json::json!({"current": {"time": "yesterday"}})).is_err());
    }
}
