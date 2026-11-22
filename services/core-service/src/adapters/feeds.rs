//! Background refresh loops. Live mode only for Open-Meteo and PetaBencana (in MOCK_MODE the
//! generator provides those inputs); always for the partner feeds (real or mock upstream).
//! A failed refresh keeps the last good data and marks that upstream stale (`meta.stale`).

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use super::http::Upstream;
use super::partners::Partners;
use super::{open_meteo, petabencana};
use crate::config::UpstreamConfig;
use crate::engine::Engine;

fn every(secs: u64) -> tokio::time::Interval {
    let mut i = tokio::time::interval(Duration::from_secs(secs));
    i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    i
}

pub fn spawn_live_inputs(engine: Arc<Engine>, up: Upstream, cfg: &UpstreamConfig) {
    let (e, u, base, secs) = (
        engine.clone(),
        up.clone(),
        cfg.open_meteo_url.clone(),
        cfg.weather_refresh_s,
    );
    tokio::spawn(async move {
        let mut t = every(secs);
        loop {
            t.tick().await;
            let res = open_meteo::fetch(&u, &base).await;
            let now = Utc::now();
            match res {
                Ok(w) => e.update(now, |world| {
                    world.set_weather(w, now);
                    world.mark_upstream(open_meteo::SERVICE, None);
                }),
                Err(err) => {
                    tracing::warn!(error = %err, "open-meteo refresh failed; keeping last weather");
                    let until = now + chrono::Duration::seconds(secs as i64 + 60);
                    e.update(now, |world| {
                        world.mark_upstream(open_meteo::SERVICE, Some(until))
                    });
                }
            }
        }
    });

    let (e, u, base, secs) = (
        engine,
        up,
        cfg.petabencana_url.clone(),
        cfg.disaster_refresh_s,
    );
    tokio::spawn(async move {
        let mut t = every(secs);
        loop {
            t.tick().await;
            let res = petabencana::fetch(&u, &base).await;
            let now = Utc::now();
            match res {
                Ok(signals) => e.update(now, |world| {
                    world.sync_external_disasters(signals, now);
                    world.mark_upstream(petabencana::SERVICE, None);
                }),
                Err(err) => {
                    tracing::warn!(error = %err, "petabencana refresh failed; keeping last signals");
                    let until = now + chrono::Duration::seconds(secs as i64 + 60);
                    e.update(now, |world| {
                        world.mark_upstream(petabencana::SERVICE, Some(until))
                    });
                }
            }
        }
    });
}

/// Mocked partners refresh every 30 s so their changes show up; real ones every
/// `PARTNER_REFRESH_SECONDS` (default 5 min, the agreed cache TTL for the destinations feed).
pub fn spawn_partner_feeds(partners: Arc<Partners>, up: Upstream, cfg: &UpstreamConfig) {
    let live_secs = cfg.partner_refresh_s;
    for which in [super::partners::TOURISM, super::partners::HEALTH] {
        let (p, u) = (partners.clone(), up.clone());
        let mocked = if which == super::partners::TOURISM {
            p.tourism_cfg.mock || p.tourism_cfg.base_url.is_none()
        } else {
            p.health_cfg.mock || p.health_cfg.base_url.is_none()
        };
        let secs = if mocked { 30.min(live_secs) } else { live_secs };
        tokio::spawn(async move {
            let mut t = every(secs);
            loop {
                t.tick().await;
                if which == super::partners::TOURISM {
                    p.refresh_tourism(&u, Utc::now()).await;
                } else {
                    p.refresh_health(&u, Utc::now()).await;
                }
            }
        });
    }
}
