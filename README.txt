KOTASIAGA LAYANAN INTEGRASI (Tugas 3: Integrasi Layanan & Platform)
=====================================================================
TST x PWAM, class K02, Smart City group KotaSiaga. Test data only.

  Layanan Integrasi (new service, off-STB):  https://kotasiaga.arkk.dev/api
  KotaSiaga microservice (STB, Tugas 2):     https://2bintang1hati.stei.site/api
  Repository:                                https://github.com/snailsquid/KotaSiaga
  Source of the new service:                 services/integration-service/
  Contract of the new service (OpenAPI 3):   services/integration-service/openapi.yaml
                                             (also served at /api/openapi.yaml)
  KotaSiaga contract:                        openapi.yaml (v1.1.0)

The mobile app (PWAM, React Native / Expo, in mobile/) talks only to the Layanan
Integrasi. The Layanan Integrasi combines three services:

  1. KotaSiaga (ours, on the STB): risk per 500 m cell, weather, safe routes,
     citizen reports, accounts, alerts.
  2. JelajahLokal (Tourism group): published tourist destinations.
  3. DonorKita (Health group): scheduled blood donation events.

What the combined service adds:
  - Tourist destinations with their current flood / road risk ("is it safe to go
    to Gedung Sate now?") -> GET /api/monitor-points
  - Donation events with risk, free slots, needed blood types
    -> GET /api/destination-points
  - A safe route to a donation event or a destination by its id, avoiding risky
    cells -> POST /api/routes/safe {"destination_id": "dnr_evt_1"}
  - One base URL, one login and one resilience policy for the app.


1. ARCHITECTURE
---------------

                      Mobile app (Expo, PWAM)
                      Authorization: Bearer <KotaSiaga JWT>
                                 |
                                 v  HTTPS
        +---------------------------------------------------------+
        | Layanan Integrasi  (Rust, Tokio + Axum)  128 MB, 0.5 CPU|
        | Coolify on volta, https://kotasiaga.arkk.dev   NOT STB  |
        |                                                         |
        |  partner feeds (memory, last good copy, stale flag)     |
        |  risk per partner point (memory, refreshed every 30 s)  |
        |  KotaSiaga read cache (fallback while KotaSiaga is down)|
        |  rate limit per client IP, one log line per call        |
        +-----+----------------------+----------------------+-----+
              | X-API-Key (ours)     | X-API-Key (theirs)   | X-API-Key (theirs)
              v                      v                      v
   KotaSiaga on the STB      JelajahLokal (Tourism)    DonorKita (Health)
   2bintang1hati.stei.site   chitatomigoreng.stei.site donorkita.stei.site
   gateway -> core-service   GET /feed/destinations    GET /donation-events
   + identity-service
   (Docker, Cloudflare tunnel; microservice containers only)

Why off the STB: the brief (slide 11, 23) allows only microservice containers on
the STB. The integration layer runs on another server (Coolify on volta), in its
own compose file (docker-compose.integration.yml), with its own Dockerfile
(services/integration-service/Dockerfile). docker-compose.yml (the STB stack) is
unchanged.

Anti-corruption layer: partner payloads are read only by the shared crate
services/partner-contracts (parsers, mocks in the partners' exact format, and the
real sample payloads used by the contract tests). Unknown fields are dropped,
strings are capped and cleaned of control characters, points outside Bandung
Raya (lat -7.10..-6.75, lon 107.45..107.80) are dropped, and any text that looks
like an instruction is just data.

Background loops refresh the partner feeds (every 5 min live, every 30 s mocked)
and the risk of every partner point (every 30 s). Requests read memory only, so a
slow partner never makes the app slow.


2. ENDPOINTS OF THE LAYANAN INTEGRASI (base https://kotasiaga.arkk.dev/api)
--------------------------------------------------------------------------

Auth: app users send "Authorization: Bearer <jwt>" from /auth/login (checked
against KotaSiaga /auth/me, cached 60 s). Server-to-server callers send an
"X-API-Key" issued by this service (API_KEYS). A caller's credentials are never
sent to the partners: the service uses its own keys for them.

  Method  Path                    Auth     Purpose
  GET     /health                 public   liveness {"status":"ok"}
  GET     /openapi.yaml           public   contract of this service
  GET     /integration/status     public   mode (live/mock), freshness and last
                                           error per upstream; no secrets
  GET     /monitor-points         jwt|key  JelajahLokal destinations + KotaSiaga
                                           risk. Filters: bbox, risk_level,
                                           category, limit (1..200), offset
  GET     /destination-points     jwt|key  DonorKita events + KotaSiaga risk.
                                           Filters: bbox, risk_level, limit, offset
  POST    /routes/safe            jwt|key  {origin, destination | destination_id,
                                           alternatives 1..3}. destination_id is a
                                           dnr_... or tour_... id from the lists
  GET     /status /risk /weather  jwt|key  KotaSiaga pass-through (same query and
          /disasters /reports              answer); last good answer kept and
          /changes                         served with meta.stale=true while
                                           KotaSiaga is down
  POST    /auth/register /login   public   KotaSiaga accounts (pass-through)
          /refresh /logout
  GET     /auth/me                jwt      the logged-in user
  POST    /reports                jwt      create a report (never retried)
  POST    /reports/{id}/votes     jwt      confirm / dispute (never retried)
  GET/POST /subscriptions,        jwt      area subscriptions and alerts
  DELETE  /subscriptions/{id},
  GET     /alerts
  POST    /_mock/scenario         admin    MOCK_MODE only: {"name":"upstream_down",
  GET     /_mock/state                     "upstream":"tourism"|"health"|
                                           "kotasiaga","seconds":30} or
                                           {"name":"recover"}

Envelope (same as the whole cluster):
  {"data": ..., "meta": {"seq", "updated_at", "source": "live"|"mock",
                         "stale": bool, "total", "sources": {...}}}
  {"error": {"code", "message", "request_id"}}
  Codes: invalid_api_key / invalid_token / invalid_credentials 401, forbidden 403,
  not_found 404, validation_error 422, rate_limited 429 (Retry-After),
  upstream_unavailable 503 (Retry-After).

Example (one monitor point):
  {"id": "tour_1", "source_id": "1", "name": "Gedung Sate",
   "category": "culture_history", "lat": -6.9025, "lon": 107.6188,
   "updated_at": "2026-11-28T00:00:00Z",
   "cell_id": "c_43_37", "risk_level": "safe", "rain_mm_h": 2.0}

Example (one destination point):
  {"id": "dnr_evt_1", "source_id": "evt_1", "kind": "donation_event",
   "name": "Donor Darah UDD PMI Kota Bandung", "organizer": "UDD PMI Kota Bandung",
   "facility_id": "fac_1", "address": "Jl. Aceh No. 79, Bandung",
   "lat": -6.9097, "lon": 107.6175,
   "opens_at": "2026-11-29T01:00:00Z", "closes_at": "2026-11-29T06:00:00Z",
   "slots_total": 40, "slots_available": 12, "capacity_per_hour": 8,
   "status": "scheduled", "needed_blood_types": ["o_negative", "ab_negative"],
   "updated_at": "2026-11-28T13:27:00Z",
   "cell_id": "c_42_37", "risk_level": "safe", "rain_mm_h": 2.0}

risk_level / cell_id / rain_mm_h are null until KotaSiaga has answered once for
that point (the app shows "Data risiko belum tersedia").

Quick try:
  BASE=https://kotasiaga.arkk.dev/api
  TOKEN=$(curl -s -H 'content-type: application/json' \
    -d '{"email":"penguji@kotasiaga.test","password":"<test password>"}' \
    "$BASE/auth/login" | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["access_token"])')
  curl -H "Authorization: Bearer $TOKEN" "$BASE/monitor-points?risk_level=safe&limit=5"
  curl -H "Authorization: Bearer $TOKEN" "$BASE/destination-points?limit=5"
  curl -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
    -d '{"origin":{"lat":-6.90,"lon":107.60},"destination_id":"dnr_evt_1"}' \
    "$BASE/routes/safe"
  curl "$BASE/integration/status"


3. ENDPOINTS CONSUMED FROM OTHER GROUPS (written agreement)
-----------------------------------------------------------

3a. JelajahLokal (Tourism)
    Group: Firanti Naulia Fasya (18224119), Rachel Elnora Mannuelle S (18224125)
    Repo:  https://github.com/elnoraam/jelajah-lokal
    Base:  https://chitatomigoreng.stei.site/api

    Call:    GET /feed/destinations?limit=200&offset=<n>
             Header X-API-Key: <key JelajahLokal issued to KotaSiaga> (TOURISM_API_KEY)
    Answer:  {"data": [{"id": "7", "name": "...", "category": "nature",
                        "lat": -6.87, "lon": 107.62,
                        "updated_at": "2026-11-27T02:15:00Z"}],
              "meta": {"seq", "updated_at", "source", "stale", "total": 12}}
             Published destinations only. category is one of nature,
             culture_history, local_culinary, tourism_village, craft.
             limit default 50, max 200; we page with offset until meta.total.
    Mapping: -> TitikPantau / monitor point, id "tour_" + id.
    Limits:  their gateway: 10 r/s per client (burst 20). We call once every
             5 min (one page per 200 destinations), cached in memory.
    Down:    last good list with meta.stale=true; with no list yet,
             /monitor-points answers 503 "Data destinasi wisata (JelajahLokal)
             belum tersedia". Everything else keeps working.
    Source checked: core-service/app/modules/feed.py.
    They call us (KotaSiaga STB): GET /risk?lat&lon and GET /weather?lat&lon with
    the X-API-Key KotaSiaga issued to them.

3b. DonorKita (Health)
    Group: Reza Nashwan Faizi (18224008)
    Repo:  https://github.com/rnfaizi/DonorKita
    Base:  https://donorkita.stei.site/api

    Call:    GET /donation-events?limit=200&offset=<n>
             Header X-API-Key: <key DonorKita issued to KotaSiaga> (HEALTH_API_KEY)
             Only this endpoint: the DonorKita owner excluded /blood-stock and
             /requests/summary from this integration.
    Answer:  {"data": [{"id": "evt_1", "name": "...", "organizer": "...",
                        "facility_id": "fac_1", "address": "...",
                        "lat": -6.8915, "lon": 107.6107,
                        "opens_at": "2026-11-29T01:00:00Z",
                        "closes_at": "2026-11-29T06:00:00Z",
                        "slots_total": 50, "slots_free": 47,
                        "capacity_per_hour": 10, "status": "scheduled",
                        "needed_blood_types": ["o_negative"],
                        "updated_at": "..."}],
              "meta": {"seq", "updated_at", "source", "stale", "total": 2}}
             Scheduled events that have not closed, ordered by opens_at.
             limit default 50, max 200; we page with offset until meta.total.
    Mapping: -> TitikTujuan / destination point, id "dnr_" + id,
             slots_free -> slots_available. Usable as destination_id in
             POST /routes/safe.
    Limits:  their gateway: 10 r/s per client (burst 20). We call once every
             5 min, cached in memory.
    Down:    last good list with meta.stale=true; with no list yet,
             /destination-points answers 503 "Data kegiatan donor (DonorKita)
             belum tersedia".
    Source checked: core-service/app/modules/drives.py (_event_out).
    They call us: POST /routes/safe and GET /status (no client on their side
    yet, 2026-11-28).

3c. KotaSiaga (our own STB microservice)
    Base:  https://2bintang1hati.stei.site/api, key KOTASIAGA_API_KEY (an entry
           "integration:<key>" in the STB's API_KEYS).
    Calls: GET /risk?lat&lon (per partner point, every 30 s, 4 in parallel),
           POST /routes/safe, GET /auth/me (token check), and the pass-through
           endpoints in section 2.
    Limits: STB gateway 20 r/s per IP (burst 40), auth 30 r/min.

Status on 2026-11-28: both partner tunnels answered Cloudflare 1033/530 (not
connected). The service is fully built against their real formats and runs on
mocks in the same formats until they are back (see section 6). Switching to live
is only .env: set the base URL and key, leave MOCK_UPSTREAM_* unset and
MOCK_MODE=false.


4. RESILIENCE (AGENTS.md section 6)
-----------------------------------
- Every outbound call: 5 s timeout, one retry after 200..600 ms jitter on
  timeout, network error or 5xx; never on 4xx; never for calls that must not run
  twice (create report, vote, register, refresh, logout). Body cap 4 MB.
- Last good copy per partner feed and per KotaSiaga read; served with
  meta.stale=true while the source is down (KotaSiaga reads: up to 60 s for
  risk/status, 10 min weather, 5 min others). Without any copy: 503 with a clear
  Indonesian message and Retry-After. The service never crashes because another
  group is down.
- Risk lookups stop at the first failure (no hammering a down KotaSiaga) and keep
  the previous values.
- Rate limit per client IP (default 600/min) with 429 + Retry-After.
- One log line per outbound attempt: timestamp, service, method, path (never the
  query string), status, ms, attempt, mode. Keys are sent in headers and never
  logged.


5. RUN LOCALLY
--------------
Needs Rust stable (or Docker). From the repo root:

  # 1) KotaSiaga itself (any of):
  #    - the real STB: KOTASIAGA_BASE_URL=https://2bintang1hati.stei.site/api
  #    - local stack: cp .env.example .env (fill CHANGE_ME, add
  #      integration:<key> to API_KEYS), then
  #      docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build
  # 2) the Layanan Integrasi:
  cp .env.integration.example .env.integration     # fill the keys
  docker compose -f docker-compose.integration.yml \
                 -f docker-compose.integration.build.yml up -d --build
  curl http://localhost:8090/api/health
  # or without Docker:
  KOTASIAGA_BASE_URL=http://localhost:8080/api KOTASIAGA_API_KEY=<key> \
  MOCK_MODE=true API_KEYS=demo:<16+ chars> ADMIN_API_KEY=<16+ chars> \
  cargo run -p integration-service

  Tests:  cargo test --workspace --locked
          npx -y @redocly/cli@1.34.5 lint services/integration-service/openapi.yaml


6. MOCK MODE AND DEMO
---------------------
MOCK_MODE=true makes both partners mocked in their exact formats (same fields,
id format, enums, envelope, limit/offset/meta.total), unless MOCK_UPSTREAM_TOURISM
/ MOCK_UPSTREAM_HEALTH say otherwise. A partner without a base URL is always
mocked. KotaSiaga is never mocked by this service: point KOTASIAGA_BASE_URL at
the STB or a local stack (which can run its own MOCK_MODE).

- Tourism mock: 12 Bandung destinations, one unpublished at a time, a different
  one every 30 s.
- Health mock: 6 donation events today (or tomorrow once closed); free slots fall
  as people reserve and refill every 10 minutes; needed blood types rotate.
- Faults: 3% of partner refreshes take one partner down for 20..40 s by itself.
  On demand (admin key):
    curl -X POST -H "X-API-Key: $ADMIN_KEY" -H 'content-type: application/json' \
      -d '{"name":"upstream_down","upstream":"health"}' "$BASE/_mock/scenario"
  -> /destination-points keeps the last list with meta.stale=true (stale banner
  in the app). {"name":"recover"} clears it.
- meta.seq rises on every change of the combined data (feed changed, risk
  changed, a source went stale or came back).


7. DEPLOY ON kotasiaga.arkk.dev (Coolify on volta, off-STB)
-----------------------------------------------------------
CI (.github/workflows/ci.yml) builds ghcr.io/snailsquid/kotasiaga-integration
(linux/amd64) next to the core images; master pushes it. In Coolify: a Docker
Compose resource from docker-compose.integration.yml, domain kotasiaga.arkk.dev
-> service integration-service, port 8090, and these environment variables
(.env.integration; never committed):

  KOTASIAGA_BASE_URL   https://2bintang1hati.stei.site/api
  KOTASIAGA_API_KEY    the key in the STB's API_KEYS for this service
  TOURISM_BASE_URL     https://chitatomigoreng.stei.site/api
  TOURISM_API_KEY      key JelajahLokal issued to KotaSiaga
  HEALTH_BASE_URL      https://donorkita.stei.site/api
  HEALTH_API_KEY       key DonorKita issued to KotaSiaga
  API_KEYS             name:key pairs for server-to-server callers (optional)
  ADMIN_API_KEY        admin key for /_mock/* (optional; MOCK_MODE only)
  MOCK_MODE            false for live (true for the demo)
  optional: MOCK_UPSTREAM_TOURISM, MOCK_UPSTREAM_HEALTH, MOCK_SEED,
            UPSTREAM_TIMEOUT_MS (5000), PARTNER_REFRESH_SECONDS (300),
            RISK_REFRESH_SECONDS (30), RATE_LIMIT_PER_MINUTE (600), RUST_LOG

Never deploy this file on the STB.


8. EVIDENCE (bukti), localhost, 2026-11-28
------------------------------------------
Tests (cargo test --workspace --locked): core-service 85 passed (1 ignored perf
test), identity-service 10, integration-service 20, partner-contracts 8,
auth-token 5; 0 failed. The contract tests feed the parsers the real JelajahLokal
and DonorKita payload shapes (services/partner-contracts/fixtures/) and run the
whole service against fake KotaSiaga / partner servers on localhost (checking
limit=200, X-API-Key, slots_free -> slots_available, destination_id routing,
stale fallback, auth and rate limits).

Outbound call log (integration-service against a local KotaSiaga in MOCK_MODE):

2026-11-28T13:27:03.926960Z  INFO outbound: service="tourism" method="GET" path="/feed/destinations" status="200" ms=0 attempt=1 mode="mock"
2026-11-28T13:27:03.927405Z  INFO outbound: service="health" method="GET" path="/donation-events" status="200" ms=0 attempt=1 mode="mock"
2026-11-28T13:27:05.941678Z  INFO outbound: service="kotasiaga" method="GET" path="/api/risk" status="200" ms=10 attempt=1 mode="live"
2026-11-28T13:27:31.863057Z  INFO outbound: service="health" method="GET" path="/donation-events" status="simulated_down" ms=0 attempt=1 mode="mock"

Live partner URLs (both tunnels down on 2026-11-28): one retry, then the
partner is marked stale and the endpoint answers a clear 503:

2026-11-28T13:27:43.202744Z  INFO outbound: service="health" method="GET" path="/api/donation-events" status="network_error" ms=38 attempt=1 mode="live"
2026-11-28T13:27:43.407062Z  INFO outbound: service="health" method="GET" path="/api/donation-events" status="network_error" ms=2 attempt=2 mode="live"
2026-11-28T13:27:43.607437Z  INFO outbound: service="tourism" method="GET" path="/api/feed/destinations" status="530" ms=442 attempt=1 mode="live"
2026-11-28T13:27:44.083655Z  INFO outbound: service="tourism" method="GET" path="/api/feed/destinations" status="530" ms=74 attempt=2 mode="live"
$ curl -H "X-API-Key: $KEY" http://127.0.0.1:18090/api/monitor-points
{"error":{"code":"upstream_unavailable","message":"Data destinasi wisata (JelajahLokal) belum tersedia","request_id":"..."}}

Docker (docker-compose.integration.yml, local build): the container used
3.6 MiB of its 128 MiB limit. It reached the real STB, which answered 401
because no KotaSiaga key was set, so risk stayed null with meta.stale=true.


9. CONTACTS
-----------
  KotaSiaga (K02):   GitHub Issues at https://github.com/snailsquid/KotaSiaga
  JelajahLokal:      Firanti Naulia Fasya (18224119), Rachel Elnora Mannuelle S
                     (18224125); https://github.com/elnoraam/jelajah-lokal
  DonorKita:         Reza Nashwan Faizi (18224008);
                     https://github.com/rnfaizi/DonorKita
  API keys are exchanged privately, never in an issue, a chat group or git.
