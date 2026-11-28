# KotaSiaga: Smart City API (TST × PWAM, K02)

Kondisi kota dan rute aman untuk Bandung Raya. Citizens report floods, accidents, damaged or
closed roads and crowds. Other citizens confirm or dispute them. Verified reports, rain (Open-Meteo)
and official disaster signals (PetaBencana) give every 500 m cell a risk level: **Aman / Waspada / Bahaya**
(`safe` / `caution` / `danger`). Safe routes avoid the risky cells, and subscribers get alerts for their areas.
Partner data comes in through an adapter and gets the same risk: Tourism destinations and Health
donation events.

| | |
|---|---|
| Base URL (STB microservice, K02) | `https://2bintang1hati.stei.site/api` |
| Layanan Integrasi (Tugas 3, off-STB, used by the mobile app) | `https://kotasiaga.arkk.dev/api`, see [README.txt](README.txt) |
| API docs (Swagger UI) | `https://2bintang1hati.stei.site/api/docs/` (local: `http://localhost:8080/api/docs/`) |
| Contract (OpenAPI 3.0.3) | [`openapi.yaml`](openapi.yaml), also served at `/api/openapi.yaml`. Standalone render: [`docs/api/KotaSiaga-API.html`](docs/api/KotaSiaga-API.html), [`docs/api/KotaSiaga-API.pdf`](docs/api/KotaSiaga-API.pdf) |
| Contract status | **FINAL** since v1.0.0 (2026-11-22); now **v1.1.0** (additive, 2026-11-28). Only additive changes |
| Auth | Group to group: `X-API-Key`. App users: `Authorization: Bearer <jwt>` |
| Test account | `penguji@kotasiaga.test` (see [Getting access](#getting-access)) |

Rules for contributors and coding agents: [AGENTS.md](AGENTS.md). The mobile app (Expo) is in [mobile/](mobile/README.md).

## Architecture

```
                 Mobile app (Expo, PWAM)            Other groups' servers
                 Bearer JWT                          X-API-Key (one key per group)
                          \                          /
                           v                        v
     Internet --> Cloudflare tunnel / Traefik --> gateway (Nginx 1.28)            64 MB, 0.25 CPU
                                                  rate limit 20 r/s per IP (auth 30 r/min)
                                                  /api/docs (Swagger UI), /api/openapi.yaml
                                     /api/auth/*  |               |  /api/*
                                                  v               v
                              identity-service (Rust)        core-service (Rust, Tokio + Axum)    256 MB, 0.5 CPU
                              64 MB, 0.25 CPU                 +----------------------------------+
                              users, Argon2id,                | Pelaporan Insiden   (reports_*)  |
                              JWT (HS256), refresh            | Verifikasi&Reputasi (verification_*)
                              tokens, test account            | Risiko Wilayah      (in memory)  |
                                  |                           | Rute Aman           (per request)|
                              identity.db                     | Peringatan & Umpan  (alerts_*)   |
                              (SQLite WAL)                    | Adaptor Data Eksternal (ACL)     |
                                                              +---------------+------------------+
                                                                 core.db      |  outbound HTTPS: 5 s timeout,
                                                                 (SQLite WAL) |  1 retry, last-good cache
                                                                              v
                       Open-Meteo (weather)   PetaBencana (disaster signals)   OSRM (road routes)
                       JelajahLokal / Tourism (destinations -> TitikPantau)
                       DonorKita / Health (donation events -> TitikTujuan)
```

**Tugas 3: Layanan Integrasi.** The mobile app does not call the STB directly. It calls the
off-STB integration service (`services/integration-service`, Coolify on volta,
`https://kotasiaga.arkk.dev/api`), which combines this API with JelajahLokal
(`GET /feed/destinations`) and DonorKita (`GET /donation-events`): destinations and donation events
with KotaSiaga risk, and safe routes to them by id. It has its own Dockerfile, compose file
(`docker-compose.integration.yml`) and contract (`services/integration-service/openapi.yaml`), and
never runs on the STB. Architecture, endpoints, agreements and evidence: [README.txt](README.txt).

```
  Mobile app --> Layanan Integrasi (off-STB) --+--> KotaSiaga on the STB (this API)
                 kotasiaga.arkk.dev            +--> JelajahLokal  chitatomigoreng.stei.site
                                               +--> DonorKita     donorkita.stei.site
```

- **Six bounded contexts** from the DDD paper. Identitas & Akses is its own container with its own
  database. The other five are modules of `core-service`. Each module owns its tables, and modules
  talk through domain events (`LaporanDibuat`, `LaporanTerverifikasi`, `LaporanKedaluwarsa`,
  `TingkatRisikoBerubah`, `CuacaDiperbarui`, `SinyalBencanaMuncul`, `SinyalBencanaSelesai`).
- **One writer, many readers.** All domain state lives in memory behind one writer. Each change
  publishes an immutable snapshot, so reads never wait and never recompute risk. Data that app users
  create (reports, votes, reputation, subscriptions, alerts) is written to SQLite through an ordered
  write-behind queue and loaded back at startup. Mock-generated data stays in memory.
- **Anti-corruption layer.** Only `services/core-service/src/adapters/` talks to outside APIs. It turns
  their payloads into our domain types (`KondisiCuaca`, `SinyalBencana`, `RuteKandidat`, `TitikPantau`,
  `TitikTujuan`). Upstream text is untrusted: only known fields are read, strings are length-capped,
  and points outside Bandung Raya are dropped.
- **STB rules.** Only microservice containers, each with `mem_limit` and `cpus`, all with
  `restart: unless-stopped`. No database port is exposed. Secrets live in `.env`. Measured idle
  memory on the local stack: core 3 MB, identity 1.4 MB, gateway 3 MB.

## Endpoints

All paths are relative to the base URL. Auth: `key` = `X-API-Key` (or a user JWT), `jwt` = user
token only, `pub` = none, `admin` = admin key. The full schemas are in [`openapi.yaml`](openapi.yaml).

| Method and path | Auth | Purpose |
|---|---|---|
| `GET /health` | pub | Liveness: `{"status":"ok"}` |
| `GET /status?lat&lon&radius_m` | key | One call for partners: cell risk, weather, active verified reports and disaster signals nearby |
| `GET /risk?lat&lon` or `?bbox=` | key | Risk of one cell, or of every cell in a box (at most 2500 cells) |
| `GET /weather?lat&lon` | key | Current weather and a 6 hour forecast (Open-Meteo) |
| `GET /disasters?bbox&limit&offset` | key | Official disaster signals (PetaBencana), active then recently resolved |
| `GET /reports?category&status&bbox&since&limit&offset` | key | Citizen reports. Incidents and events are reports filtered by `category` |
| `GET /changes?since=<seq>&limit` | key | Change feed for live updates (last 500 changes, `meta.reset` when you fell behind) |
| `POST /routes/safe` | key | Up to 3 routes scored by risk, recommended first. `destination` or `destination_id` |
| `GET /monitor-points?bbox&risk_level` | key | Tourism destinations (TitikPantau) with the current risk at each |
| `GET /destination-points?bbox&risk_level` | key | Health donation events (TitikTujuan) with risk, usable as `destination_id` |
| `POST /auth/register` | pub | Create an app user (test data only) |
| `POST /auth/login` | pub | Email + password to access token (1 h) + refresh token (30 d) |
| `POST /auth/refresh` | pub | Single-use refresh token to a new pair |
| `POST /auth/logout` | pub | Revoke a refresh token |
| `GET /auth/me` | jwt | The logged-in user |
| `POST /reports` | jwt | Create a report. It starts `waiting` and never raises risk until verified. 10 per user per hour |
| `POST /reports/{id}/votes` | jwt | Body `{"vote":"confirm"}` or `{"vote":"dispute"}`, weighted by the voter's reputation (0.2 to 1.0) |
| `GET /subscriptions`, `POST /subscriptions` | jwt | Area subscriptions (max 20 per user, radius 100 to 5000 m) with the area's current risk |
| `DELETE /subscriptions/{id}` | jwt | Remove one of your subscriptions and its alerts |
| `GET /alerts?subscription_id&limit&offset` | jwt | Alerts raised when the worst risk level inside one of your areas changes |
| `GET /_mock/state`, `POST /_mock/scenario` | admin | Demo controls, only with `MOCK_MODE=true` (otherwise `404`) |
| `GET /docs/`, `GET /openapi.yaml` | pub | Swagger UI and the raw contract (served by the gateway) |

**Conventions.** JSON with `snake_case` keys and lowercase enum values. Times are ISO 8601 UTC with
`Z`. Distances are in meters, durations in seconds, temperatures in °C. Coordinates are WGS84
`lat`/`lon`, and `bbox` is `min_lon,min_lat,max_lon,max_lat`. The service area is lat -7.10 to
-6.75, lon 107.45 to 107.80. A point outside it returns `422`. Lists take `limit` (default 50,
max 200) and `offset`.

```json
{ "data": { }, "meta": { "seq": 1042, "updated_at": "2026-11-22T06:57:28Z", "source": "live", "stale": false } }
{ "error": { "code": "invalid_api_key", "message": "API key tidak valid", "request_id": "..." } }
```

- `meta.source` is `live`, or `mock` when the data comes from the generator or a mocked partner.
  `meta.stale` is `true` while some upstream is down and cached data is served. `meta.seq` rises
  on every data change, so poll `/changes?since=<seq>`.
- Error codes: `invalid_api_key` / `invalid_token` / `invalid_credentials` 401, `forbidden` 403,
  `not_found` 404, `conflict` 409, `validation_error` 422, `rate_limited` 429 (with `Retry-After`),
  `upstream_unavailable` 503.

### Quick start

```sh
BASE=https://2bintang1hati.stei.site/api  # or http://localhost:8080/api
KEY=<your group key>

curl "$BASE/health"
curl -H "X-API-Key: $KEY" "$BASE/status?lat=-6.9025&lon=107.6188&radius_m=1000"
curl -H "X-API-Key: $KEY" -H 'content-type: application/json' \
  -d '{"origin":{"lat":-6.9025,"lon":107.6188},"destination":{"lat":-6.8945,"lon":107.5985}}' \
  "$BASE/routes/safe"

TOKEN=$(curl -s -H 'content-type: application/json' \
  -d '{"email":"penguji@kotasiaga.test","password":"<test account password>"}' \
  "$BASE/auth/login" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["access_token"])')
curl -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"category":"flood","lat":-6.9175,"lon":107.6093,"description":"Genangan 20 cm","severity":2}' \
  "$BASE/reports"
curl -H "X-API-Key: $KEY" "$BASE/changes?since=0&limit=50"   # then poll with since=<meta.seq>
```

[Appendix A](#appendix-a-sample-requests-and-responses) has a `curl` command and a real response for **every**
endpoint. They were captured from the local stack and each one passed the schema check.

## Getting access

1. **Group API key.** Each group gets its own key so it can be revoked alone. Ask the KotaSiaga
   team: open a GitHub Issue in this repo titled `API key request: <group name>`, or contact
   `<KotaSiaga contact: name, WhatsApp/LINE, email>`. Say your group name, class and theme. The
   key is sent privately, never in an issue or a chat group. Send it as `X-API-Key: <key>`.
2. **Test account** (Tugas Besar: "satu akun uji"): email `penguji@kotasiaga.test`, display name
   *Akun Uji KotaSiaga*. Its password is sent together with your API key. It lives only in the
   server's `.env` (`TEST_ACCOUNT_PASSWORD`) and is never committed. The account is re-seeded at
   every start. You can also register your own test users at `/auth/register`. Use test data only.
3. **Limits.** Per client IP, the gateway allows 20 requests/s (burst 40) on `/api/*` and 30
   requests/min (burst 10) on `/api/auth/*`. Above that it answers `429` with `Retry-After`. Please
   cache on your side (suggested TTL: weather 10 min, risk 60 s, status 60 s).

## Integration agreements

This section is the written agreement the brief asks for ("kesepakatan tertulis"). It is mirrored in
the partner groups' READMEs. Partner payload shapes were checked against their source code
(DonorKita `core-service/app/modules/drives.py`, JelajahLokal `core-service/app/modules/feed.py`)
and are pinned by the contract tests in `services/partner-contracts`. Both adapters (core-service
here and the Layanan Integrasi) use the same crate. On 2026-11-28 both partner tunnels returned
Cloudflare 1033, so both run on mocks in the partners' exact formats until they are back.

### Health: DonorKita (`https://donorkita.stei.site/api`)

| | DonorKita calls KotaSiaga | KotaSiaga calls DonorKita |
|---|---|---|
| Endpoints | `POST /routes/safe` (safe route to a donation site, `destination_id` from `/destination-points` or coordinates), `GET /status?lat&lon&radius_m` (road and weather around a site), `GET /risk?lat&lon` | `GET https://donorkita.stei.site/api/donation-events?limit=200&offset=N` with `X-API-Key`, **only this endpoint** (their owner excluded `/blood-stock` and `/requests/summary`) |
| Data format | Section [Endpoints](#endpoints) and `openapi.yaml` (envelope `data` + `meta`) | `{data: [{id: "evt_1", name, organizer, facility_id, address, lat, lon, opens_at, closes_at, slots_total, slots_free, capacity_per_hour, status, needed_blood_types[], updated_at}], meta: {seq, updated_at, source, stale, total}}`; `limit` max 200, so we page until `meta.total`. Each event becomes `TitikTujuan` `dnr_<id>`, with `slots_free` as `slots_available` |
| Auth | `X-API-Key` issued by KotaSiaga to DonorKita | `X-API-Key` issued by DonorKita to KotaSiaga (`HEALTH_API_KEY`) |
| Rate limit | 20 r/s per IP at our gateway; please keep 5 r/s or less and cache routes 60 s | Their gateway: 10 r/s (burst 20). We call once every 5 min (`PARTNER_REFRESH_SECONDS`) |
| When the other side is down | Use your cache. We answer `503 upstream_unavailable` with `Retry-After` only if OSRM is down and no cached route exists | We keep the last good list (`meta.stale: true`). With no data, `/destination-points` answers `503` with a clear message. The rest of the API is unaffected |
| Status | Contract final (v1.1.0) | CONFIRMED from their source. Tunnel down on 2026-11-28 (Cloudflare 1033); a mock in their exact format runs meanwhile (`MOCK_UPSTREAM_HEALTH`) |
| Contact | Reza Nashwan Faizi (18224008), [`rnfaizi/DonorKita`](https://github.com/rnfaizi/DonorKita) | GitHub Issues in this repo |

Open mismatch: our DDD paper says Health provides "fasilitas kesehatan dan jam buka". DonorKita
actually exposes donation events, stock and request counts, so the adapter maps donation events to
`TitikTujuan`. The paper wording still has to be aligned.

### Tourism: JelajahLokal (`https://chitatomigoreng.stei.site/api`)

| | JelajahLokal calls KotaSiaga | KotaSiaga calls JelajahLokal |
|---|---|---|
| Endpoints | `GET /status?lat&lon&radius_m` (risk, weather and reports around a destination), `GET /weather?lat&lon`, `GET /risk?lat&lon` or `?bbox=` | `GET https://chitatomigoreng.stei.site/api/feed/destinations?limit=200&offset=N` with `X-API-Key` (published destinations only) |
| Data format | Section [Endpoints](#endpoints) and `openapi.yaml` | `{data: [{id: "7", name, category, lat, lon, updated_at}], meta: {..., total}}`, `category` one of `nature`, `culture_history`, `local_culinary`, `tourism_village`, `craft`; `limit` max 200, so we page until `meta.total`. Each one becomes `TitikPantau` `tour_<id>`, served with risk at `/monitor-points` |
| Auth | `X-API-Key` issued by KotaSiaga to JelajahLokal | `X-API-Key` issued by JelajahLokal to KotaSiaga (`TOURISM_API_KEY`) |
| Rate limit | 20 r/s per IP at our gateway; please cache `/status` 60 s and `/weather` 10 min | Their gateway: 10 r/s (burst 20). Once every 5 min (their feed's agreed cache TTL) |
| When the other side is down | Use your cache, show "Data risiko belum tersedia" | Last good list with `meta.stale: true`, or `503` on `/monitor-points` only |
| Status | Contract final (v1.1.0) | CONFIRMED from their source. Tunnel down on 2026-11-28 (530 / Cloudflare 1033); a mock in their exact format (12 Bandung destinations, one unpublished at a time, changing every 30 s) runs meanwhile (`MOCK_UPSTREAM_TOURISM`) |
| Contact | Firanti Naulia Fasya (18224119), Rachel Elnora Mannuelle S (18224125), [`elnoraam/jelajah-lokal`](https://github.com/elnoraam/jelajah-lokal) | GitHub Issues in this repo |

### Public APIs (no key)

| API | Used for | Call | Refresh |
|---|---|---|---|
| Open-Meteo | `KondisiCuaca`: rain and temperature, 6 h forecast | `GET /v1/forecast?latitude=-6.9175&longitude=107.6191&current=...&hourly=...` | every 10 min |
| PetaBencana | `SinyalBencana`: flood, fire, wind, haze, earthquake and volcano reports in West Java (last 6 h) | `GET /reports?admin=ID-JB&timeperiod=21600&geoformat=geojson` | every 5 min |
| OSRM (public demo server) | `RuteKandidat`: up to 3 driving alternatives | `GET /route/v1/driving/{lon},{lat};{lon},{lat}?alternatives=3&overview=full&geometries=geojson` | per request, cached 10 min per point pair |

## Resilience

Every outbound call (`adapters/http.rs`) has a 5 s timeout and **one retry** after 200 to 600 ms
of jitter, on a timeout, network error or 5xx. A 4xx is not retried. Bodies are capped at 4 MB, and
each attempt writes **one log line** (`target: outbound`: service, method, path, status, ms,
attempt, `mode=live|mock`). When a refresh fails, the last good data stays in use and the upstream
is marked stale, so responses carry `meta.stale: true`. With no data at all, the endpoint answers
`503 upstream_unavailable` with a clear Indonesian message. Other endpoints keep working. The
gateway re-resolves service names, so recreating one container does not break routing. While a
service restarts, the gateway answers a JSON `503` with `Retry-After`.

## Mock mode (demo)

`MOCK_MODE=true` randomizes the **inputs** only: weather, reports, votes, disaster signals and
upstream faults. Verification, risk scoring, alerts, routing and the change feed run the real code.
`meta.seq` advances on every tick and risk levels change by themselves. Set `MOCK_SEED` to replay
the same story. Routes use synthetic road shapes unless `OSRM_IN_MOCK=true`. Tourism and Health
are mocked in their exact formats whenever their base URL is unset, and in `MOCK_MODE` by default:
`MOCK_UPSTREAM_TOURISM` / `MOCK_UPSTREAM_HEALTH` follow `MOCK_MODE` when unset; set one to `false`
to call that live partner in mock mode, or `true` to mock it in live mode. The fault injector can
also take `tourism` or `health` down, so the partner stale path can be demoed.

```sh
curl -X POST -H "X-API-Key: $ADMIN_KEY" -H 'content-type: application/json' \
  -d '{"name":"flood_verified","lat":-6.885,"lon":107.613}' "$BASE/_mock/scenario"
```

Scenarios: `rain_storm`, `flood_verified`, `road_closure`, `event_crowd`, `disaster_signal`,
`upstream_down` (optional `"upstream"`: `open_meteo` (default), `petabencana`, `osrm`, `tourism`,
`health`), `recover`, `reset`. `reset` clears only mock data (generated reports, their votes,
mock users and disaster signals). Reports and votes from app users stay, both in memory and in SQLite.

## Evidence (localhost, 2026-11-28, Tugas 3)

- `cargo test --workspace --locked`: core-service 85 passed (1 ignored perf test), identity-service
  10, integration-service 20, partner-contracts 8, auth-token 5. 0 failed.
- `scripts/validate_responses.py` against the Docker stack in `MOCK_MODE` (images built locally):
  ALL OK, and [docs/samples.md](docs/samples.md) was regenerated from that run (partner points now in
  the partners' real formats: `tour_1` / `culture_history`, `dnr_evt_1` with `slots_available`).
- `scripts/validate_integration.py` against the Layanan Integrasi running in front of that stack:
  10/10 ok. Login, `/auth/me`, `/reports`, `/status`, `/subscriptions`, `/alerts` and a route to
  `destination_id: tour_4` worked through it with a user JWT. More in [README.txt](README.txt).

## Evidence (localhost, 2026-11-22)

All of the following came from `docker compose` on a developer machine (images built locally with
`docker-compose.build.yml`, same Dockerfiles as CI).

**End-to-end contract check.** `scripts/validate_responses.py` calls every endpoint, including
error cases, and validates each body against `openapi.yaml` (unknown fields rejected). Live mode:
35/35 ok (the four `/_mock` checks are skipped). Mock mode: 39/39 ok.

```text
ok  POST   /auth/login                       -> 200 (68 ms) log in with the test account
ok  POST   /routes/safe                      -> 200 (597 ms) safe route
ok  POST   /reports                          -> 201 (5 ms) create a report
ok  POST   /reports/rpt_1/votes              -> 403 (2 ms) vote on your own report
ok  POST   /reports/rpt_1/votes              -> 200 (2 ms) confirm someone else's report
ok  POST   /reports/rpt_1/votes              -> 409 (2 ms) vote twice
ok  GET    /alerts?limit=5                   -> 200 (14 ms) alerts after a flood near the area
ALL OK (0 bad)
```

**Outbound call log, live mode** (`docker compose logs core-service | grep outbound`):

```text
2026-11-22T06:54:54.195696Z  INFO outbound: service="petabencana" method="GET" path="/reports" status="200" ms=140 attempt=1 mode="live"
2026-11-22T06:54:55.071139Z  INFO outbound: service="open_meteo" method="GET" path="/v1/forecast" status="200" ms=1016 attempt=1 mode="live"
2026-11-22T06:56:28.926556Z  INFO outbound: service="tourism" method="GET" path="/feed/destinations" status="200" ms=0 attempt=1 mode="mock"
2026-11-22T06:56:44.018939Z  INFO outbound: service="osrm" method="GET" path="/route/v1/driving/107.618800,-6.902500;107.598500,-6.894500" status="200" ms=562 attempt=1 mode="live"
2026-11-22T06:56:44.019309Z  INFO access: method=POST path="/api/routes/safe" status=200 ms=563 caller="tourism"
2026-11-22T06:58:08.698766Z  INFO outbound: service="health" method="GET" path="/donation-events" status="200" ms=0 attempt=1 mode="mock"
```

**Upstream down** (PetaBencana pointed at a closed port): one retry, then the last good data with `meta.stale: true`.

```text
2026-11-22T06:56:53.628880Z  INFO outbound: service="petabencana" method="GET" path="/reports" status="network_error" ms=0 attempt=1 mode="live"
2026-11-22T06:56:54.035393Z  INFO outbound: service="petabencana" method="GET" path="/reports" status="network_error" ms=0 attempt=2 mode="live"
2026-11-22T06:56:54.035534Z  WARN core_service::adapters::feeds: petabencana refresh failed; keeping last signals
$ curl -H "X-API-Key: $KEY" "$BASE/disasters?limit=1"
{"data":[],"meta":{"seq":1,"updated_at":"2026-11-22T06:56:54.361701663Z","source":"live","stale":true}}
```

**Live updates** (`MOCK_MODE=true`): two calls 10 s apart, `meta.seq` 18 then 34.
**Persistence:** after `docker compose up --force-recreate core-service`, the log shows
`stored data loaded live_reports=1`, and the user's report and its vote count are still there.
**Backup:** `scripts/backup.sh` gave `backup /data/backup.db integrity=ok` for both databases.

## Develop

```sh
cargo test --locked                                   # all services (unit + HTTP tests)
cargo clippy --all-targets --locked -- -D warnings
npx -y @redocly/cli@1.34.5 lint openapi.yaml
npx -y @redocly/cli@1.34.5 lint services/integration-service/openapi.yaml
scripts/sync_openapi.sh                               # after editing openapi.yaml (a test checks the copy)

# Full stack on localhost (never build on a deploy host):
cp .env.example .env                                  # then replace every CHANGE_ME
docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build --pull never
curl http://localhost:8080/api/health

# Contract check against the running stack (pip install jsonschema pyyaml):
python3 scripts/validate_responses.py openapi.yaml http://localhost:8080/api \
  <group key> <admin key> penguji@kotasiaga.test <test password> [--samples docs/samples.md]

# Layanan Integrasi (off-STB) on localhost, in front of the stack above:
cp .env.integration.example .env.integration          # set KOTASIAGA_BASE_URL (STB or a host address the container can reach) and keys
docker compose -f docker-compose.integration.yml -f docker-compose.integration.build.yml up -d --build
curl http://localhost:8090/api/health

# Regenerate README.pdf and docs/api/* (needs: pip install markdown playwright; playwright install chromium)
python3 scripts/build_docs.py
```

Main environment variables (see [`.env.example`](.env.example)): `API_KEYS` (`name:key` pairs),
`ADMIN_API_KEY`, `JWT_SECRET` (same value for both services, at least 32 characters),
`TEST_ACCOUNT_EMAIL` / `TEST_ACCOUNT_PASSWORD`, `MOCK_*`, `OSRM_IN_MOCK`, and the upstream URLs and
keys (`TOURISM_BASE_URL`, `TOURISM_API_KEY`, `HEALTH_BASE_URL`, `HEALTH_EVENTS_PATH`, `HEALTH_API_KEY`).
Without `JWT_SECRET`, both services still start, but login answers `503` and user endpoints answer `401`.

## Backup

Never copy only the `.db` file (WAL mode). `scripts/backup.sh` asks each service binary to make a
consistent online copy (`/app backup <dest> [--overwrite]` runs `VACUUM INTO` and then `PRAGMA
integrity_check`). It works in the distroless image, which has no shell. The binary refuses a
destination that is the live database or one of its `-wal`/`-shm` files, and refuses an existing
destination unless `--overwrite` is given. The script finds the containers by their compose labels,
copies the result out with `docker cp`, and keeps the newest 14 per service. It runs on any Docker
host shell: an STB (project `kotasiaga`) or the Coolify host on volta (project = the app's UUID; set
`PROJECT` when more than one stack is running):

```sh
scripts/backup.sh                          # -> backups/core-<UTC time>.db, backups/identity-<UTC time>.db
PROJECT=<coolify app uuid> scripts/backup.sh /var/backups/kotasiaga     # on volta, over SSH
# daily at 03:15 with cron:
15 3 * * * PROJECT=kotasiaga /path/to/KotaSiaga/scripts/backup.sh /var/backups/kotasiaga >> /var/backups/kotasiaga/backup.log 2>&1
```

Coolify's own "scheduled task" cannot run it, because the containers have no shell. Use cron on the
host, or run `docker exec <core container> /app backup /data/backup.db --overwrite` as the scheduled
command and copy `/data/backup.db` off the volume.

To restore: stop the service, put the backup in the volume as `core.db` / `identity.db`, and
remove any stale `-wal`/`-shm` files next to it.

## Deploy

CI (`.github/workflows/ci.yml`) runs on Gitea Actions on volta. It tests and builds the
`linux/amd64` images on every pushed branch. On `master` it also pushes them to `ghcr.io/snailsquid`
and triggers Coolify, which runs `docker-compose.yml` (no `build:` keys: the deploy host only
pulls). The STB runs the same `docker-compose.yml` with `COMPOSE_PROFILES=tunnel` and
`TUNNEL_TOKEN`, which starts the `cloudflared` container for `2bintang1hati.stei.site`.
`kotasiaga.arkk.dev` (volta's Cloudflare tunnel and Coolify's Traefik) is the off-STB **Layanan
Integrasi**: a separate Coolify resource from `docker-compose.integration.yml`, image
`ghcr.io/snailsquid/kotasiaga-integration` (built by the same CI, pushed on master), port 8090.
Its environment variables are listed in [`.env.integration.example`](.env.integration.example) and
README.txt section 7. Add an `integration:<key>` entry to the STB's `API_KEYS` and set the same key as
its `KOTASIAGA_API_KEY`. CI and the Coolify setup are maintained
separately (DevOps). Before this version reaches master, set `JWT_SECRET` and
`TEST_ACCOUNT_PASSWORD` in the Coolify app env.

```sh
cp .env.example .env              # fill every CHANGE_ME (and TUNNEL_TOKEN on an STB)
docker compose pull && docker compose up -d
curl http://127.0.0.1:8080/api/health
```

ARM STB: not supported until CI builds `linux/arm64` images again (GHCR has amd64 only).

**Secrets.** Both services refuse to start when `JWT_SECRET`, `ADMIN_API_KEY`, an `API_KEYS` entry or
`TEST_ACCOUNT_PASSWORD` is still a `.env.example` placeholder (`CHANGE_ME...`), or when two groups
share a key. Generate each value with `openssl rand -hex 32`.

**Client IP and rate limits.** The gateway takes the client IP from `CF-Connecting-IP` only when the
request comes from a container on one of its own Docker networks (the tunnel or Traefik). The list is
generated at container start; add more with `TRUSTED_PROXY_CIDRS`. The proxy in front (Traefik on
volta) must only accept traffic that came through the Cloudflare tunnel. Otherwise a client that
reaches it directly can set the header and dodge the per-IP limits.

**Shutdown.** On SIGTERM, core-service stops its ticker and waits up to 10 s for queued database
writes. `POST /reports` and votes only answer after their rows are committed, so an acknowledged
write survives a redeploy.

**Token lifetimes.** Logout revokes the refresh token only. An access token stays valid until it
expires (at most 1 hour). Refresh tokens are single use, but reusing an old one does not revoke the
newer ones from the same login.

## Contract changes in v1.1.0 (2026-11-28, additive)

- `servers` is now the STB: `https://2bintang1hati.stei.site/api`.
- `POST /_mock/scenario` takes an optional `upstream` for `upstream_down` (`open_meteo`,
  `petabencana`, `osrm`, `tourism`, `health`).
- `/monitor-points` and `/destination-points` examples and descriptions follow the partners' real
  formats (`tour_7`, JelajahLokal categories, `dnr_evt_1`, `slots_available` = DonorKita
  `slots_free`). No field, path or enum changed.

## Contract changes in v1.0.0 (for Tourism and Health)

All additive except the version bump to 1.0.0, which marks the contract final:

- New: `POST /auth/refresh`, `POST /auth/logout`, `GET /auth/me`; `Token` now also has
  `refresh_token`, `refresh_expires_in` and `user`.
- New: `DELETE /subscriptions/{id}`; `Subscription.risk_level`; `Alert.previous_level`; `GET
  /alerts?subscription_id=`.
- New: `GET /monitor-points`, `GET /destination-points`; `POST /routes/safe` also accepts
  `destination_id` (then `destination` is optional).
- New error codes: `invalid_token`, `invalid_credentials` (401) and `conflict` (409, duplicate
  email, double vote, vote on a closed report).
- Read endpoints and `/routes/safe` accept a user JWT as an alternative to `X-API-Key`.

## Appendix A: sample requests and responses

<!-- samples:start -->
See [docs/samples.md](docs/samples.md). `scripts/build_docs.py` inlines it here in README.pdf.
<!-- samples:end -->
