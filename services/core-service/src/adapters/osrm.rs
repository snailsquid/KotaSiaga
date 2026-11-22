//! OSRM -> RuteKandidat. Asks for up to 3 driving alternatives; Rute Aman then scores them
//! against the risk grid. Road geometry barely changes, so answers are cached for 10 minutes
//! and the last good answer is served (stale) when OSRM is down.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::http::{FetchError, Upstream, log_call};
use crate::routing::CandidateRoute;
use crate::rules::MOCK_ROUTE_SPEED_M_S;

pub const SERVICE: &str = "osrm";
const FRESH_FOR_S: i64 = 600;
const CACHE_CAP: usize = 256;
const MAX_POINTS: usize = 2000;

pub type Point = (f64, f64); // (lat, lon)
type Cache = HashMap<String, (Vec<CandidateRoute>, DateTime<Utc>)>;

pub fn url(base: &str, o: Point, d: Point, alternatives: u8) -> String {
    format!(
        "{base}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?alternatives={}&overview=full&geometries=geojson&steps=false",
        o.1,
        o.0,
        d.1,
        d.0,
        if alternatives > 1 {
            alternatives.to_string()
        } else {
            "false".into()
        }
    )
}

pub fn parse(v: &Value) -> Result<Vec<CandidateRoute>, FetchError> {
    let bad = |m: &str| FetchError::Invalid(format!("osrm: {m}"));
    match v.get("code").and_then(Value::as_str) {
        Some("Ok") => {}
        Some("NoRoute") => return Ok(Vec::new()),
        other => return Err(bad(&format!("code {other:?}"))),
    }
    let routes = v
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("no routes"))?;
    Ok(routes
        .iter()
        .take(3)
        .filter_map(|r| {
            let coords: Vec<(f64, f64)> = r
                .pointer("/geometry/coordinates")?
                .as_array()?
                .iter()
                .take(MAX_POINTS)
                .filter_map(|p| {
                    let p = p.as_array()?;
                    let (lon, lat) = (p.first()?.as_f64()?, p.get(1)?.as_f64()?);
                    (lon.is_finite() && lat.is_finite()).then_some((lon, lat))
                })
                .collect();
            (coords.len() >= 2).then_some(CandidateRoute {
                distance_m: r.get("distance")?.as_f64()?.max(0.0),
                duration_s: r.get("duration")?.as_f64()?.max(0.0),
                coords,
            })
        })
        .collect())
}

/// Synthetic candidates for MOCK_MODE: the straight line plus two curved detours either side.
pub fn mock_candidates(o: Point, d: Point, alternatives: u8) -> Vec<CandidateRoute> {
    let offsets = [0.0, 0.22, -0.22];
    offsets
        .iter()
        .take(alternatives.clamp(1, 3) as usize)
        .map(|&k| {
            // Quadratic Bezier with the control point pushed sideways by k * length.
            let (mlat, mlon) = ((o.0 + d.0) / 2.0, (o.1 + d.1) / 2.0);
            let (dlat, dlon) = (d.0 - o.0, d.1 - o.1);
            let (clat, clon) = (mlat - dlon * k, mlon + dlat * k);
            let n = 24;
            let coords: Vec<(f64, f64)> = (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    let a = (1.0 - t) * (1.0 - t);
                    let b = 2.0 * (1.0 - t) * t;
                    let c = t * t;
                    let lat = a * o.0 + b * clat + c * d.0;
                    let lon = a * o.1 + b * clon + c * d.1;
                    (lon, lat)
                })
                .collect();
            let distance_m = crate::routing::polyline_length_m(&coords);
            CandidateRoute {
                distance_m,
                duration_s: distance_m / MOCK_ROUTE_SPEED_M_S,
                coords,
            }
        })
        .collect()
}

pub struct Candidates {
    pub routes: Vec<CandidateRoute>,
    /// Served from cache because OSRM failed.
    pub stale: bool,
}

pub struct RouteSource {
    up: Upstream,
    base: String,
    /// Live OSRM (`false` = synthetic candidates, MOCK_MODE without OSRM_IN_MOCK).
    live: bool,
    cache: Mutex<Cache>,
}

impl RouteSource {
    pub fn new(up: Upstream, base: String, live: bool) -> Self {
        Self {
            up,
            base,
            live,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn key(o: Point, d: Point, n: u8) -> String {
        format!("{:.4},{:.4};{:.4},{:.4};{n}", o.0, o.1, d.0, d.1)
    }

    pub async fn candidates(&self, o: Point, d: Point, n: u8) -> Result<Candidates, FetchError> {
        if !self.live {
            let started = Instant::now();
            let routes = mock_candidates(o, d, n);
            log_call(
                SERVICE,
                "/route/v1/driving",
                "200",
                started.elapsed().as_millis() as u64,
                1,
                "mock",
            );
            return Ok(Candidates {
                routes,
                stale: false,
            });
        }
        let key = Self::key(o, d, n);
        let now = Utc::now();
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned();
        if let Some((routes, at)) = &cached
            && now - *at < Duration::seconds(FRESH_FOR_S)
        {
            return Ok(Candidates {
                routes: routes.clone(),
                stale: false,
            });
        }
        let fetched = self
            .up
            .get_json(SERVICE, &url(&self.base, o, d, n), &[])
            .await
            .and_then(|v| parse(&v));
        match fetched {
            Ok(routes) => {
                let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                if c.len() >= CACHE_CAP {
                    let oldest = c
                        .iter()
                        .min_by_key(|(_, (_, t))| *t)
                        .map(|(k, _)| k.clone());
                    if let Some(k) = oldest {
                        c.remove(&k);
                    }
                }
                c.insert(key, (routes.clone(), now));
                Ok(Candidates {
                    routes,
                    stale: false,
                })
            }
            Err(e) => match cached {
                Some((routes, _)) => {
                    tracing::warn!(error = %e, "osrm down, serving cached routes");
                    Ok(Candidates {
                        routes,
                        stale: true,
                    })
                }
                None => Err(e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_osrm_routes() {
        let v = serde_json::json!({"code": "Ok", "routes": [
            {"distance": 5400.2, "duration": 780.5, "geometry": {"type": "LineString", "coordinates": [[107.60, -6.90], [107.61, -6.905], [107.62, -6.91]]}},
            {"distance": 6000.0, "duration": 700.0, "geometry": {"type": "LineString", "coordinates": [[107.60, -6.90]]}}
        ]});
        let r = parse(&v).unwrap();
        assert_eq!(r.len(), 1, "a one-point geometry is dropped");
        assert_eq!(r[0].coords[1], (107.61, -6.905));
        assert!(parse(&serde_json::json!({"code": "InvalidQuery"})).is_err());
        assert!(
            parse(&serde_json::json!({"code": "NoRoute"}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn mock_candidates_differ_and_end_at_the_points() {
        let (o, d) = ((-6.90, 107.60), (-6.91, 107.62));
        let r = mock_candidates(o, d, 3);
        assert_eq!(r.len(), 3);
        for c in &r {
            assert_eq!(c.coords.first(), Some(&(o.1, o.0)));
            let last = c.coords.last().unwrap();
            assert!((last.0 - d.1).abs() < 1e-9 && (last.1 - d.0).abs() < 1e-9);
        }
        assert!(
            r[1].distance_m > r[0].distance_m,
            "a detour is longer than the straight line"
        );
    }

    #[test]
    fn url_is_lon_lat_order() {
        let u = url("https://osrm.test", (-6.9, 107.6), (-6.91, 107.62), 3);
        assert!(u.starts_with("https://osrm.test/route/v1/driving/107.600000,-6.900000;107.620000,-6.910000?alternatives=3"));
    }
}
