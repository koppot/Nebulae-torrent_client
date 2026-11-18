# syntax=docker/dockerfile:1
FROM debian:bookworm-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src ./src
RUN cargo build --release --locked

# ── Runtime image ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    transmission-daemon \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false -d /data nebulae \
    && mkdir -p /data/downloads /data/config /data/watch \
    && chown -R nebulae:nebulae /data

COPY --from=builder /build/target/release/nebulae-server /usr/local/bin/nebulae-server
COPY server/docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh /usr/local/bin/nebulae-server

# Data volumes: downloads, Transmission config, watch directory
VOLUME ["/data/downloads", "/data/config", "/data/watch"]

# nebulae-server REST API port
EXPOSE 9174
# Transmission peer port
EXPOSE 51413
EXPOSE 51413/udp

ENTRYPOINT ["/entrypoint.sh"]
