# KotaSiaga: Smart City service (TST × PWAM)

Kondisi kota dan rute aman untuk Bandung: citizen reports, verification, per-cell risk
(Aman / Waspada / Bahaya), weather, disaster signals and safe routes. Rules for every
contributor and agent are in [AGENTS.md](AGENTS.md). The API contract is [openapi.yaml](openapi.yaml).

The mobile app (Expo, frontend only for now) lives in [mobile/](mobile/README.md).

## Stack

Rust (Tokio + Axum), SQLite in WAL mode, Nginx gateway, Cloudflare tunnel, Docker on the group STB.

| Container | Role | Limits |
|---|---|---|
| `core-service` | 5 bounded contexts as modules | 256 MB, 0.5 CPU |
| `identity-service` | user JWT, group API keys | 64 MB, 0.25 CPU |
| `gateway` | Nginx routing, rate limit (429 + `Retry-After`) | 64 MB, 0.25 CPU |
| `tunnel` | Cloudflare tunnel to `https://<name>.stei.site` | 64 MB, 0.25 CPU |

## Endpoints

Base: `https://<name>.stei.site/api` (name pending in the API Registry). Contract: **PROPOSED**, see
`openapi.yaml`. Group keys go in the `X-API-Key` header.

| Method and path | Auth | Implemented |
|---|---|---|
| `GET /health` | none | yes |
| `GET /status?lat&lon&radius_m` | key | yes |
| `GET /risk?lat&lon` or `?bbox=` | key | yes |
| `GET /weather?lat&lon` | key | yes (mock data; Open-Meteo adapter pending) |
| `GET /disasters?bbox&limit&offset` | key | yes (mock data; PetaBencana adapter pending) |
| `GET /reports?category&status&bbox&since&limit&offset` | key | yes |
| `GET /changes?since&limit` | key | yes |
| `POST /_mock/scenario`, `GET /_mock/state` | admin key, `MOCK_MODE=true` only | yes |
| `POST /routes/safe` | key | no |
| `POST /auth/register`, `/auth/login` | none | no |
| `POST /reports`, `/reports/{id}/votes`, `/subscriptions`; `GET /subscriptions`, `/alerts` | JWT | no |

```sh
BASE=https://<name>.stei.site/api KEY=<your group key>
curl "$BASE/health"
curl -H "X-API-Key: $KEY" "$BASE/status?lat=-6.89&lon=107.61&radius_m=1000"
curl -H "X-API-Key: $KEY" "$BASE/risk?lat=-6.89&lon=107.61"
curl -H "X-API-Key: $KEY" "$BASE/changes?since=0&limit=50"   # then poll with since=<meta.seq>
```

### Mock mode (demo)

`MOCK_MODE=true` randomizes the inputs only (weather, reports, votes, disaster signals, upstream
faults). Verification, risk scoring and the change feed run the real code, so `meta.seq` advances
every tick and risk levels change by themselves. Set `MOCK_SEED` to replay the same story.

```sh
curl -X POST -H "X-API-Key: $ADMIN_KEY" -H 'content-type: application/json' \
  -d '{"name":"flood_verified","lat":-6.885,"lon":107.613}' "$BASE/_mock/scenario"
```

Scenarios: `rain_storm`, `flood_verified`, `road_closure`, `event_crowd`, `disaster_signal`,
`upstream_down`, `recover`, `reset`.

## Develop

```sh
cargo test                     # all services
cargo clippy --all-targets -- -D warnings
npx -y @redocly/cli@1.34.5 lint openapi.yaml
MOCK_MODE=true API_KEYS=dev:devkey-0000000000 ADMIN_API_KEY=admin-0000000000 \
  DATABASE_PATH=./core.db PORT=8080 cargo run -p core-service
# Check live responses against the contract (needs: pip install jsonschema pyyaml)
python3 scripts/validate_responses.py openapi.yaml http://127.0.0.1:8080/api devkey-0000000000 admin-0000000000
```

## Deploy (Volta host or STB)

CI (`.github/workflows/ci.yml`) runs on **Gitea Actions on volta**: Gitea pull-mirrors this repo
every 10 minutes and its runner executes `.github/workflows`. It tests and builds the
`linux/amd64` images on every pushed branch (build only, nothing pushed); on `master` it also pushes
them to `ghcr.io/snailsquid` (secret `GHCR_TOKEN`, a classic PAT with `write:packages`) and deploys.
It can also be started by hand (`workflow_dispatch`). Master runs queue and are never cancelled, so
an older run cannot overwrite a newer `:latest`; a new push to any other branch cancels that
branch's older run. arm64 images for an ARM STB come back once GitHub Actions
works for this repo; the workflow stays GitHub-compatible.
Never compile Rust on the deploy host: `docker-compose.yml` has no `build:` keys on purpose.

### volta (Coolify)

Coolify on volta runs this repo as a **git-based** Docker Compose app (`kotasiaga`, branch
`master`, file `/docker-compose.yml`). It has to be git-based because the gateway bind-mounts
`./gateway/nginx.conf` from the checkout. Coolify only sees compose changes once they are on `master`.

After the images are pushed, the `deploy` job runs on the runner labelled `volta`, calls
Coolify's deploy API, waits for that Coolify deployment to finish, then polls the health URL for up
to 5 minutes and fails if it never returns `{"status":"ok"}`. The health endpoint has no version
field, so the check proves the stack serves traffic, not which image runs.
`core-service` and `identity-service` use `pull_policy: always`, so each deploy pulls `:latest`.
These repo secrets are needed (while any is unset the job is skipped with a warning in the log and
in the run summary, and nothing is deployed):

| Secret | Value |
|---|---|
| `COOLIFY_URL` | Coolify base URL as seen from the volta runner |
| `COOLIFY_TOKEN` | Coolify API token with deploy permission |
| `COOLIFY_RESOURCE_UUID` | UUID of the `kotasiaga` compose app |
| `HEALTH_URL` (optional, variable or secret) | health check URL, default `https://kotasiaga.arkk.dev/api/health` |

Env vars are set in the Coolify app, which writes them to the `.env` the services read: `IMAGE_REGISTRY`, `IMAGE_TAG`, `RUST_LOG`,
`MOCK_*` and `GATEWAY_PORT=18080` (8080 belongs to Traefik there). `COMPOSE_PROFILES` stays unset,
so the `tunnel` container does not start. Public traffic reaches the gateway through the Coolify
domain `kotasiaga.arkk.dev`: volta's wildcard `*.arkk.dev` Cloudflare tunnel points at Coolify's
Traefik, which reaches the gateway over the compose network. The loopback `GATEWAY_PORT` publish
is only for local checks on the host.

### STB (manual)

Before you start:

- **ARM STB: not supported right now.** CI publishes `linux/amd64` images only, so on an ARM
  host `docker compose pull`/`up` fails (no matching manifest). This returns when the arm64 leg
  comes back with GitHub Actions. An x86 STB works.
- **GHCR must be reachable.** `core-service` and `identity-service` use `pull_policy: always`
  (Coolify needs it to pick up `:latest`), so `docker compose up` fails when GHCR cannot be
  reached, even with the images already pulled. Offline, run `docker compose up -d --pull never`.
- **Existing STB `.env`: add `COMPOSE_PROFILES=tunnel` before pulling this compose file.** The
  `tunnel` service now only starts under that profile; without it the STB loses its public URL.

```sh
cp .env.example .env           # fill TUNNEL_TOKEN; keep COMPOSE_PROFILES=tunnel; never commit .env
docker compose pull && docker compose up -d
curl http://127.0.0.1:8080/api/health
```

`COMPOSE_PROFILES=tunnel` starts the `cloudflared` container, which reaches the gateway over the
compose network.

### Local build (never on a deploy host)

```sh
docker compose -f docker-compose.yml -f docker-compose.build.yml up --build
```

Back up SQLite with `VACUUM INTO` or the backup API, not by copying only the `.db` file (WAL mode).
