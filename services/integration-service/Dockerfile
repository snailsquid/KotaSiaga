# syntax=docker/dockerfile:1.7
# Layanan Integrasi (Tugas 3). Runs OFF the STB (Coolify on volta, https://kotasiaga.arkk.dev).
# Built by CI (linux/amd64) and pulled by the deploy host; never compiled there.
# Context is the repo root (the service uses the shared workspace crates):
#   docker build -f services/integration-service/Dockerfile -t kotasiaga-integration .
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY services ./services
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p integration-service \
 && cp target/release/integration-service /app

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /app /app
USER nonroot
EXPOSE 8090
ENTRYPOINT ["/app"]
