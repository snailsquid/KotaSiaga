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

Base: `https://<name>.stei.site/api` (name pending in the API Registry). Status: **PROPOSED**,
only `/health` is implemented so far.

| Method and path | Auth | Implemented |
|---|---|---|
| `GET /health` | none | yes |
| `GET /status`, `/risk`, `/weather`, `/disasters`, `/reports`, `/changes` | `X-API-Key` or JWT | no |
| `POST /routes/safe` | `X-API-Key` or JWT | no |
| `POST /auth/register`, `/auth/login` | none | no |
| `POST /reports`, `/reports/{id}/votes`, `/subscriptions`; `GET /subscriptions`, `/alerts` | JWT | no |

```sh
curl https://<name>.stei.site/api/health
# {"status":"ok"}
```

## Develop

```sh
cargo test                     # all services
cargo clippy --all-targets -- -D warnings
npx -y @redocly/cli@1.34.5 lint openapi.yaml
DATABASE_PATH=./core.db PORT=8080 cargo run -p core-service
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
