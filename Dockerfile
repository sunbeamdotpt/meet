# syntax=docker/dockerfile:1.7
# sunbeam-meet-server — multi-stage build
#
# Stage 1: cargo-chef recipe (dependency graph)
# Stage 2: cargo-chef cook (compile deps, cacheable layer)
# Stage 3: build release binary
# Stage 4: distroless runtime
#
# Runtime uses distroless/cc-debian12 because several deps (sqlx/native-tls via
# transitive crates, aws-lc-sys for rustls-aws-lc-rs, libpq alternatives) may
# pull in glibc + a C runtime. If the Implementer locks everything to pure
# rustls-ring + rustls-native-roots disabled, switch to distroless/static.

ARG RUST_VERSION=1.82
ARG DEBIAN_VERSION=bookworm

# ---------- chef base ----------
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS chef
RUN cargo install cargo-chef --locked --version ^0.1
WORKDIR /app

# ---------- planner ----------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- cacher: build deps only ----------
FROM chef AS cacher
# protoc is needed because sunbeam-meet-proto has build.rs driving tonic-build
# + connect-build (DESIGN §2). Install upfront so the dep-cook layer is stable.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      protobuf-compiler \
      pkg-config \
      libssl-dev \
      cmake \
      ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# ---------- builder: project ----------
FROM chef AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      protobuf-compiler \
      pkg-config \
      libssl-dev \
      cmake \
      ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY . .
RUN cargo build --release --bin sunbeam-meet-server \
 && strip target/release/sunbeam-meet-server

# ---------- runtime ----------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Nonroot UID is 65532 in this image.
USER nonroot:nonroot
WORKDIR /app

COPY --from=builder --chown=nonroot:nonroot \
     /app/target/release/sunbeam-meet-server \
     /usr/local/bin/sunbeam-meet-server

# 8080 → Connect / gRPC / gRPC-Web (single port, per DESIGN §2 / PLAN)
# 9090 → Prometheus /metrics (DESIGN §10)
EXPOSE 8080 9090

# The Implementer exposes /healthz on the main HTTP listener (8080) — keep in
# sync with crates/sunbeam-meet-server/src/main.rs.
# Distroless has no shell, so we use the direct exec form. k8s liveness/readiness
# probes in deploy/base/deployment.yaml are authoritative; this HEALTHCHECK is
# for local `docker run` only.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/sunbeam-meet-server", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/sunbeam-meet-server"]
