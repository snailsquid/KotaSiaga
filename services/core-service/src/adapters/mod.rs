//! Adaptor Data Eksternal (anti-corruption layer). The only place that talks to outside APIs:
//! Open-Meteo, PetaBencana, OSRM, Tourism (JelajahLokal) and Health (DonorKita).
//! Each adapter turns the upstream format into our domain types, so nothing else ever sees a raw
//! upstream payload. Every outbound call goes through `http::Upstream`: 5 s timeout, one retry
//! with jitter on timeout/5xx, and one `outbound` log line per attempt (the integration evidence).

pub mod feeds;
pub mod http;
pub mod open_meteo;
pub mod osrm;
pub mod partners;
pub mod petabencana;
