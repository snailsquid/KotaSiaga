# syntax=docker/dockerfile:1.7
# Builds one service binary. Build on an arm64 machine (CI runner), never on the STB.
#   docker build --build-arg SERVICE=core-service -t kotasiaga-core .
ARG SERVICE=core-service

FROM rust:1-slim-bookworm AS build
ARG SERVICE
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY services ./services
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p ${SERVICE} \
 && cp target/release/${SERVICE} /app
# /data must exist in the image, owned by nonroot: Docker seeds an empty named volume with the
# mount point's ownership, so without this the volume is root-owned and SQLite cannot create
# its file. Distroless has no shell, so the directory is made here and copied over.
RUN mkdir -p /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /app /app
COPY --from=build --chown=nonroot:nonroot /data /data
USER nonroot
ENTRYPOINT ["/app"]
