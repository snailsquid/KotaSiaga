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
Never compile Rust on the deploy host: `docker-compose.yml` has no `build:` keys on purpose.

### volta (Coolify)

Coolify on volta runs this repo as a **git-based** Docker Compose app (`kotasiaga`, branch
`master`, file `/docker-compose.yml`). It has to be git-based because the gateway bind-mounts
`./gateway/nginx.conf` from the checkout. Coolify only sees compose changes once they are on `master`.

After the images are pushed, the `deploy` job runs on the self-hosted runner on volta
(labels `self-hosted, linux, x64, volta, kotasiaga`) and calls Coolify's deploy API.
`core-service` and `identity-service` use `pull_policy: always`, so each deploy pulls `:latest`.
GitHub environment `volta` holds these secrets; the job skips with a warning while any is unset:

| Secret | Value |
|---|---|
| `COOLIFY_URL` | Coolify base URL as seen from the volta runner |
| `COOLIFY_TOKEN` | Coolify API token with deploy permission |
| `COOLIFY_RESOURCE_UUID` | UUID of the `kotasiaga` compose app |

Env vars are set in the Coolify app, which writes them to the `.env` the services read: `IMAGE_REGISTRY`, `IMAGE_TAG`, `RUST_LOG`,
`MOCK_*` and `GATEWAY_PORT=18080` (8080 belongs to Traefik there). `COMPOSE_PROFILES` stays unset,
so the `tunnel` container does not start. Public traffic reaches the gateway through the Coolify
domain `kotasiaga.arkk.dev`: volta's wildcard `*.arkk.dev` Cloudflare tunnel points at Coolify's
Traefik, which reaches the gateway over the compose network. The loopback `GATEWAY_PORT` publish
is only for local checks on the host.

### STB (manual)

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
