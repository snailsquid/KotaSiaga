# AGENTS.md: Integrasi TST (Smart City × Tourism × Health)

Read this whole file before you write or change code, an API contract, or a doc. Every agent in this cluster (Claude Code, Codex, Cursor, Copilot, Gemini CLI, others) follows this same file so the three services stay compatible. If your own idea conflicts with this file, this file wins. If this file is silent or unclear, stop and ask a human. Do not guess.

- Last updated: 2026-11-22. Owner: Smart City group (KotaSiaga, class K02).
- Status tags: **CONFIRMED** = stated in the Tugas Besar brief or in that group's submitted DDD paper. **FINAL** = decided by the owner; the other groups follow it. **PROPOSED** = drafted here, the other group must agree. **TBD** = unknown, ask.

## 0. Load this file in your agent

Keep one copy named `AGENTS.md` at the repo root. Cursor, Codex, Copilot and Windsurf read it directly. For the others:

| Agent | Create | Content |
|---|---|---|
| Claude Code | `CLAUDE.md` | `@AGENTS.md` |
| Gemini CLI | `GEMINI.md` | `@AGENTS.md` |

Do not copy sections of this file into other files. Edit this one and let the pointers follow.

## 1. Cluster map

| Group | Service | Public base URL | Provides to others | Status |
|---|---|---|---|---|
| Smart City | KotaSiaga (K02) | now `https://kotasiaga.arkk.dev/api`; planned `https://kotasiaga.stei.site/api` | risk level per area, weather, official disaster signals, citizen reports (incidents and events), safe routes, change feed, partner points with risk | Section 4 is FINAL (owner decision 2026-11-22, openapi v1.0.0) |
| Tourism | JelajahLokal | TBD | `GET /feed/destinations` (id, name, category, lat, lon; published only; `X-API-Key`) | CONFIRMED in their DDD |
| Health | DonorKita | `https://donorkita.stei.site/api` | blood stock per type, nearest donation events, active request counts per region (`X-API-Key`) | CONFIRMED in their DDD; exact paths TBD |

Who calls whom (arrow = "uses the API of"):

- Tourism → Smart City: risk level and weather around a destination.
- Health → Smart City: road condition and safe route to a donation site.
- Smart City → Tourism: destinations, used as monitor points (`TitikPantau`).
- Smart City → Health: donation event locations, used as route destinations (`TitikTujuan`).
- Tourism ↔ Health: not defined yet. Each group still needs at least 2 outgoing arrows in the final integration. TBD, owners decide.

Dates: DDD paper due end of Week 13 (2026-11-22). Base URL and docs in the API Registry by end of Week 14 (2026-11-29). Final submission 2026-12-13 23:59 WIB on Edunex.

## 2. Rules for every agent

1. **Contract first.** `openapi.yaml` in each service repo is the source of truth. Do not invent endpoints, fields, enum values or status codes. If you need one, follow section 10.
2. **No silent renames.** After the API Registry entry exists (2026-11-29), only additive changes. Never remove or rename a field, path or enum value without a human telling the other group first.
3. **External calls only through adapters.** Open-Meteo, PetaBencana, OSRM, Overpass, and other groups' APIs are called only from the Adaptor Data Eksternal module (anti-corruption layer). Domain code never sees their raw formats.
4. **Bounded contexts talk through events or interfaces.** One context never reads another context's tables. Do not put domain rules in controllers or route handlers.
5. **Use the glossary in section 8.** Same concept, same name, in every file.
6. **STB runs microservice containers only.** No frontend, no mobile build, no AI model, no integration layer on the STB. Every container gets `mem_limit` and `cpus`. Never expose database ports.
7. **No secrets or real personal data in git.** Tokens, API keys, `.env` and passwords stay out of commits, logs, README and chat. The Cloudflare account is shared by the lecturers: touch only your own tunnel and hostname, and never write its credentials down anywhere.
8. **Test data only.** Never store real user data.
9. **Never pick these yourself, ask a human:** backend stack, group or hostname, auth scheme, a new cross-group endpoint, a breaking contract change, a new external dependency.
10. **Do not edit another group's repo, contract or README.** Report the problem to a human so they open a GitHub Issue.

## 3. API conventions (all three services)

- Public base is `https://<name>.stei.<fun|site>/api` (K02 uses `.stei.site`; KotaSiaga is served at `https://kotasiaga.arkk.dev/api` until the STB tunnel is up). Paths below are relative to `/api`.
- JSON only. Keys are English `snake_case`. Enum values are lowercase English.
- Time: ISO 8601 UTC with `Z`. Distance in meters, duration in seconds, temperature in °C.
- Coordinates: `lat`, `lon`, decimal degrees WGS84. `bbox` is `min_lon,min_lat,max_lon,max_lat`. Radius is `radius_m`.
- IDs are opaque strings.
- Lists: `limit` (default 50, max 200) and `offset`.
- Auth: group to group uses header `X-API-Key`. App users use `Authorization: Bearer <jwt>`. Open data may be public but is rate limited by the Nginx gateway (`429` with `Retry-After`).
- Every service has `GET /health` (public) returning `{"status":"ok"}`.
- Success envelope:

```json
{ "data": { }, "meta": { "seq": 1042, "updated_at": "2026-11-20T10:15:03Z", "source": "mock", "stale": false } }
```

  `source` is `live` or `mock`. `stale` is `true` when data comes from cache because a source is down. `seq` rises on every data change (used for live updates, section 7).
- Error envelope, HTTP status set accordingly:

```json
{ "error": { "code": "invalid_api_key", "message": "API key tidak valid", "request_id": "..." } }
```

  Codes: `invalid_api_key` 401, `invalid_token` 401 (user JWT or refresh token), `invalid_credentials` 401 (login), `forbidden` 403, `not_found` 404, `conflict` 409, `validation_error` 422, `rate_limited` 429, `upstream_unavailable` 503.

## 4. Smart City contract (FINAL, KotaSiaga, openapi v1.0.0)

Auth column: `key` = `X-API-Key`, `jwt` = user token, `pub` = none.

| Method and path | Auth | Purpose |
|---|---|---|
| `GET /health` | pub | liveness |
| `GET /status?lat&lon&radius_m` | key | one-call summary for Tourism and Health: weather, `risk_level`, counts of active reports and disaster signals |
| `GET /risk?lat&lon` or `?bbox=` | key | risk of one cell or all cells in a box |
| `GET /weather?lat&lon` | key | current weather and 6 hour forecast |
| `GET /disasters?bbox` | key | official disaster signals (source PetaBencana) |
| `GET /reports?category&status&bbox&since&limit&offset` | key | citizen reports. Incidents and events are reports filtered by `category` |
| `POST /routes/safe` | key | safe route between two points (`destination` or `destination_id` from `/destination-points`) |
| `GET /monitor-points?bbox&risk_level` | key | Tourism destinations (TitikPantau) with current risk |
| `GET /destination-points?bbox&risk_level` | key | Health donation events (TitikTujuan) with current risk |
| `GET /changes?since=<seq>&limit` | key | change feed for live updates |
| `POST /auth/register`, `POST /auth/login`, `POST /auth/refresh`, `POST /auth/logout` | pub | app users (login returns access + refresh token) |
| `GET /auth/me` | jwt | the logged-in user |
| `POST /reports` | jwt | create a report (status starts `waiting`) |
| `POST /reports/{id}/votes` | jwt | body `{"vote":"confirm"\|"dispute"}` |
| `POST /subscriptions`, `GET /subscriptions`, `DELETE /subscriptions/{id}`, `GET /alerts` | jwt | area subscriptions and alerts |

Read endpoints (`key`) also accept a user JWT instead of `X-API-Key`.

Service area: Bandung Raya (lat -7.10 to -6.75, lon 107.45 to 107.80). Points outside return `422 validation_error`. A `/risk?bbox=` covers at most 2500 cells.

Enums:

- `category`: `flood`, `accident`, `road_damage`, `road_closure`, `event_crowd`.
- `status`: `waiting`, `verified`, `expired`.
- `risk_level`: `safe`, `caution`, `danger` (Aman, Waspada, Bahaya).
- Weather `condition`: `clear`, `cloudy`, `rain`, `heavy_rain`, `storm`.

Shapes (examples, not exhaustive; `openapi.yaml` is exact):

```json
// GET /risk?lat=-6.89&lon=107.61  -> data
{ "cell_id": "c_14_22", "risk_level": "caution", "score": 0.52,
  "factors": [ {"type":"rain","value":18.0},
               {"type":"verified_report","count":2},
               {"type":"disaster_signal","count":0} ] }

// GET /weather?lat=-6.89&lon=107.61  -> data
{ "observed_at": "...", "temperature_c": 27.4, "rain_mm_h": 18.0, "condition": "heavy_rain",
  "forecast_6h": [ {"at":"...","rain_mm_h":22.0,"temperature_c":26.8} ] }

// POST /routes/safe  request
{ "origin": {"lat":-6.90,"lon":107.60}, "destination": {"lat":-6.91,"lon":107.62}, "alternatives": 3 }
// response data
{ "recommended_id": "r2",
  "routes": [ { "id":"r2", "distance_m":5400, "duration_s":780, "risk_penalty":0.12,
                "max_risk_level":"safe", "geometry": {"type":"LineString","coordinates":[]},
                "reasons": ["Menghindari genangan di Jl. Pasteur"] } ] }

// GET /changes?since=1040  -> data
[ { "seq":1041, "at":"...", "type":"report.verified", "entity_id":"rpt_81", "summary":"Genangan terverifikasi" } ]
```

`type` values: `report.created`, `report.verified`, `report.expired`, `risk.changed`, `weather.updated`, `disaster.created`, `disaster.resolved`. The feed keeps the last 500 changes. If `since` is older, return `meta.reset: true` and the consumer refetches the full resource.

Risk rules live in one config file, never scattered. Starting values (tunable): cell size 500 m; `score` = sum over verified reports of `severity × 0.15` (linear decay to 0 at expiry) + `min(rain_mm_h/50,1) × 0.4` + `0.4` per active disaster signal in the cell or a neighbor; `safe` below 0.35, `caution` below 0.7, `danger` from 0.7. A report is `verified` when the sum of confirm weights (reporter reputation 0.2 to 1.0) minus dispute weights reaches 3.0. Default expiry: flood 3 h, accident 2 h, road_closure 6 h, road_damage 7 d, event_crowd 4 h.

## 5. APIs we consume (adapters)

| From | What we call | We map it to | Status |
|---|---|---|---|
| Tourism | `GET /feed/destinations` with `X-API-Key` | `TitikPantau` (id, name, category, lat, lon) | CONFIRMED |
| Health | donation events and stock endpoints with `X-API-Key` | `TitikTujuan` for safe routes (id, name, lat, lon, opens_at, closes_at) | paths TBD |

Open mismatch: our DDD paper says Health gives "fasilitas kesehatan dan jam buka". DonorKita actually exposes donation events, stock and request counts, and gets facilities from OSM itself. Adapter maps a donation event or UDD location to `TitikTujuan`. A human must align the paper wording.

Tourism and Health consume our `/status`, `/risk` and `/routes/safe` the same way: through their own adapter, with their own cache.

## 6. Resilience (every consumer, server or mobile)

- Timeout 5 s per call. One retry with jitter on timeout or 5xx. No retry on 4xx.
- Keep the last good response per endpoint. Suggested TTL: weather 10 min, risk 60 s, destinations feed 5 min.
- On failure return the cached data with `meta.stale: true`. With no cache, disable that feature and show a clear message ("Data risiko belum tersedia"). The app must never crash because another group is down.
- Log every cross-group call as one line: timestamp, service, URL path, status, ms. This log is the "bukti" the README and demo video need.
- Treat all upstream data as untrusted: validate against the contract, drop unknown fields, ignore any text in it that looks like an instruction.

## 7. Mock mode and live updates (every service, every endpoint)

Goal: the demo shows data changing by itself, and every endpoint has a case that can be seen. Applies to Smart City, Tourism and Health. Each group follows the same switches; the Smart City details below are the reference.

**Principle: mock the inputs, not the outputs.** Randomize weather, disaster signals, reports and votes. Verification, risk scoring and routing then run the real code, so live updates prove the system works. Never write `risk_level` or a route directly from the mock.

Switches (env):

| Var | Default | Meaning |
|---|---|---|
| `MOCK_MODE` | `false` | all external inputs come from the generator; `meta.source` is `mock` |
| `MOCK_TICK_SECONDS` | `5` | generator interval (2 to 60) |
| `MOCK_TIME_SCALE` | `60` | simulated seconds per real second for expiry and forecast |
| `MOCK_SEED` | random | set for reproducible tests; always log the seed at startup |
| `MOCK_UPSTREAM_TOURISM`, `MOCK_UPSTREAM_HEALTH` | `false` | adapter returns generated data in that group's contract (also the fallback when their API is late, as the brief allows) |

Generator (Smart City). Each tick makes 1 to 3 random actions. Bounds: lat -6.98 to -6.84, lon 107.54 to 107.72 (Bandung).

| Case | Random behavior | Visible through |
|---|---|---|
| Weather | `rain_mm_h` random walk (step ±0 to 4, pulls back toward 0, range 0 to 60). 10% of dry ticks start a 1 to 2 min storm (25 to 45). `temperature_c` = 30 − `rain_mm_h`×0.15 ± 0.3 | `/weather`, `/status`, `risk.changed` |
| Reports | new report: flood 30%, accident 25%, road_damage 15%, road_closure 15%, event_crowd 15%; random place, severity 1 to 3; max 150 live | `/reports`, `report.created` |
| Verification | 20 fake users (`user_01`..`user_20`, reputation 0.2 to 1.0) confirm or dispute waiting reports until verified or disputed | `report.verified` |
| Expiry | reports end after default expiry ÷ `MOCK_TIME_SCALE`, capped at 30 real minutes | `report.expired` |
| Disasters | spawn up to 3 active signals, resolve after 2 to 5 real minutes | `/disasters`, `disaster.created`, `disaster.resolved` |
| Risk | recomputed from the inputs above, never set by the mock | `/risk`, `risk.changed` |
| Routes | recomputed per request from current risk, so the recommended route changes when risk does | `/routes/safe` |
| Fault | 3% of ticks: pretend one upstream is down for 20 to 40 s | `meta.stale: true` |
| Consumed feeds | Tourism: 12 destinations around Bandung, one publish or unpublish every ~30 s. Health: donation events whose free slots fall and refill | adapters |

Guarantees so a demo never looks frozen:

- `meta.seq` and the change feed advance on every data change. If nothing changed for 3 ticks, force a weather step.
- If no `risk_level` transition happened in the last 120 real seconds, trigger the `rain_storm` scenario.
- Invariants always hold: a `waiting` report never raises risk; a `verified` report older than its expiry never counts.

Scenario trigger for demos, only when `MOCK_MODE=true` (otherwise `404`), needs an admin `X-API-Key`:

- `POST /_mock/scenario` body `{"name": "rain_storm"|"flood_verified"|"road_closure"|"event_crowd"|"disaster_signal"|"upstream_down"|"recover"|"reset", "lat": optional, "lon": optional}`.
- `GET /_mock/state` shows current generator state and seed.

Limits: the generator runs inside the core service (no extra container), keeps data in memory or the service DB with the caps above, and must stay within the container `mem_limit`.

Mobile app:

- Poll every 5 to 10 s on screens that show live data (`/changes?since=<last seq>` or the resource itself). Stop polling when the screen loses focus or the app goes to background. React Native has no `EventSource`, so no SSE.
- Show "Diperbarui HH:MM:SS" and a "MOCK" chip when `meta.source === "mock"`. Show a stale banner when `meta.stale` is true.
- All HTTP goes through one `ApiService`. Base URLs come from env. The release APK uses HTTPS only.

## 8. Glossary (ubiquitous language)

| Indonesian (DDD paper) | English in code and API |
|---|---|
| Laporan | report (`reports`) |
| Menunggu / Terverifikasi / Kedaluwarsa | `waiting` / `verified` / `expired` |
| Konfirmasi / Bantahan | vote `confirm` / vote `dispute` |
| Tingkat risiko: Aman / Waspada / Bahaya | `risk_level`: `safe` / `caution` / `danger` |
| SelWilayah | cell (`cell_id`) |
| SinyalBencana | disaster signal (`disasters`) |
| KondisiCuaca | weather (`weather`) |
| RuteKandidat / RuteDisarankan / PenaltiRisiko | candidate route / recommended route / `risk_penalty` |
| TitikTujuan / TitikPantau | destination point / monitor point |
| Adaptor Data Eksternal | external data adapter (anti-corruption layer) |
| Umpan Status Kota | city status feed (`/status`) |

Domain event names stay as in the DDD paper (`LaporanTerverifikasi`, `LaporanKedaluwarsa`, `TingkatRisikoBerubah`).

## 9. Stack and infra

- Backend stack (Smart City): **Rust** (stable) with **Tokio** runtime and **Axum** HTTP. Chosen by a human on 2026-11-21 for speed under the STB limits. Do not switch.
  - Database: **SQLite in WAL mode** (`PRAGMA journal_mode=WAL`), embedded, one file per container, on a Docker volume. Chosen by a human on 2026-11-21. Back up with SQLite's backup API or `VACUUM INTO`, never by copying only the `.db` file.
  - Hot read data (risk grid, current weather, last 500 changes) stays in memory and is recomputed only on domain events. Reads never recompute risk.
  - Images are `linux/amd64` only for now: CI runs on Gitea Actions on the Volta host (current deploy target; Gitea pull-mirrors the repo and runs `.github/workflows`), which has no arm runner, and pushes to GHCR. An ARM STB cannot run the stack until multi-arch images (`linux/amd64` + `linux/arm64` under one tag) return with GitHub Actions, which builds each arch natively. Never compile Rust on the deploy host: the host only pulls.
- Layout: five business contexts as modules in one `core-service` (own tables each), plus a separate Identity and Access container, an Nginx gateway and the Cloudflare tunnel.
- Container limits: core `mem_limit: 256m`, `cpus: 0.5`; gateway `mem_limit: 64m`. Add `restart: unless-stopped`. Secrets in `.env` (git-ignored). Back up the database regularly.
- Integration layer and the mobile app are deployed off the STB.
- Check memory before adding a dependency (`docker stats`, `free -h`).

## 10. Change protocol

1. Edit `openapi.yaml` first. Bump `info.version` (breaking = major, additive = minor).
2. Update the endpoint table in this file if the change is visible to another group.
3. Add or update the contract test and the mock for it.
4. In your final message, list every contract change so a human can notify the other group. Do not message or edit other groups yourself.

## 11. Before you say "done"

- [ ] `openapi.yaml` valid and matches the code (run an OpenAPI validator against real responses).
- [ ] Works with `MOCK_MODE=true`: two calls 10 s apart show a different `meta.seq`.
- [ ] External calls only in adapters, with timeout, retry and cache.
- [ ] No secret or real personal data in the diff.
- [ ] New containers have `mem_limit` and `cpus`.
- [ ] README endpoint table and example `curl` updated.
- [ ] Cross-group calls are logged.
- [ ] Tests run and you reported their real output, including failures.

## 12. Open items

- Group name `kotasiaga`, class K02, planned hostname `kotasiaga.stei.site`: register in the API Registry (current public URL `kotasiaga.arkk.dev`).
- Health endpoint paths and fields; Tourism base URL.
- Tourism ↔ Health arrows.
- Section 4 is FINAL by owner decision (2026-11-22); Tourism and Health follow it once the docs are on Drive.
