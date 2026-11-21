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

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /app /app
USER nonroot
ENTRYPOINT ["/app"]
