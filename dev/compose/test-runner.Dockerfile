# Integration-test runner. Provides a Linux x86_64/arm64 environment with a
# working libwebrtc prebuilt so the `livekit` Rust RTC SDK actually runs —
# darwin-arm64 libwebrtc links against a system abseil whose Objective-C
# bridge is incompatible with macOS 26, crashing at PeerConnectionFactory
# init. Linux is what LiveKit's own CI targets.
#
# Built + invoked via `docker compose run --rm test-runner …` from
# dev/compose. The repo is bind-mounted at /src; `CARGO_TARGET_DIR=/target`
# writes into a named volume so the host's macOS target/ stays intact.

FROM rust:1.82-bookworm

# System deps — protobuf for tonic-build, pkg-config + openssl for rustls
# alternatives, cmake for any C++ build script livekit-ffi invokes, ca-certs
# so reqwest's webpki roots match the container's trust store.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        protobuf-compiler \
        libprotobuf-dev \
        pkg-config \
        libssl-dev \
        cmake \
        lld \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/*

# cargo-nextest — the integration profile is driven through it. The release
# host ships separate tarballs for x86_64 / aarch64 under different slugs;
# pick based on the container's arch so this image works on both Intel
# linux boxes and Apple Silicon through Lima.
RUN set -eux; \
    arch="$(uname -m)"; \
    case "$arch" in \
        x86_64)  slug="linux" ;; \
        aarch64) slug="linux-arm" ;; \
        *) echo "unsupported arch: $arch" >&2; exit 1 ;; \
    esac; \
    curl -LsSf "https://get.nexte.st/latest/$slug" | tar -xz -C /usr/local/bin

ENV CARGO_TARGET_DIR=/target \
    CARGO_HOME=/cargo \
    RUST_BACKTRACE=1 \
    PROTOC=/usr/bin/protoc \
    PROTOC_INCLUDE=/usr/include \
    CARGO_BUILD_JOBS=2 \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only \
    RUSTFLAGS="-C link-arg=-fuse-ld=lld"

WORKDIR /src
