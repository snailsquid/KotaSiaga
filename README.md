# KotaSiaga: Smart City service (TST × PWAM)

Kondisi kota dan rute aman untuk Bandung: citizen reports, verification, per-cell risk
(Aman / Waspada / Bahaya), weather, disaster signals and safe routes. Rules for every
contributor and agent are in [AGENTS.md](AGENTS.md). The API contract is [openapi.yaml](openapi.yaml).

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

CI (`.github/workflows/ci.yml`) tests every push and, on `master`, builds multi-arch images
(`linux/amd64` + `linux/arm64`, one tag) on native GitHub runners and pushes them to GHCR.
Any host (x86 server or ARM STB) runs the same compose file and Docker pulls the matching arch.
Never compile Rust on the deploy host.

```sh
cp .env.example .env           # fill TUNNEL_TOKEN and IMAGE_REGISTRY; never commit .env
docker compose pull && docker compose up -d
curl http://127.0.0.1:8080/api/health
```

Back up SQLite with `VACUUM INTO` or the backup API, not by copying only the `.db` file (WAL mode).
