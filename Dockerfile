# nebulae-server Docker image
#
# Runs Transmission daemon + nebulae-server bridge in a single container.
# The bridge exposes a bearer-token-authenticated REST API on port 9174.
#
# Build:
#   docker build -t nebulae-server .
#
# Run:
#   docker run -d --name nebulae-server \
#     -p 9174:9174 \
#     -p 51413:51413 -p 51413:51413/udp \
#     -e NEBULAE_TOKEN=your-secret-token \
#     -v nebulae-data:/data \
#     nebulae-server
#
# Environment variables:
#   NEBULAE_TOKEN          (required) Bearer token for API authentication
#   NEBULAE_PORT           (default: 9174) REST API port
#   NEBULAE_BIND           (default: 0.0.0.0) Listen address
#   TRANSMISSION_RPC_URL   (default: http://127.0.0.1:9091/transmission/rpc)
#   TRANSMISSION_USER      (default: empty) RPC username
#   TRANSMISSION_PASS      (default: empty) RPC password
#   PUID                   (default: 1000) User ID
#   PGID                   (default: 1000) Group ID

# ── Stage 1: Build nebulae-server ──────────────────────────────────────────

FROM rust:1.83-bookworm AS builder

WORKDIR /build
COPY server/ .
RUN cargo build --release --locked 2>/dev/null || cargo build --release

# ── Stage 2: Runtime ───────────────────────────────────────────────────────

FROM debian:bookworm-slim

ARG PUID=1000
ARG PGID=1000

RUN apt-get update && apt-get install -y --no-install-recommends \
    transmission-daemon \
    ca-certificates \
    tini \
    && rm -rf /var/lib/apt/lists/*

# Create user and data directories.
RUN groupadd -g ${PGID} nebulae && \
    useradd -u ${PUID} -g nebulae -m -s /bin/bash nebulae && \
    mkdir -p /data/downloads /data/watch /data/config && \
    chown -R nebulae:nebulae /data

# Copy the built binary.
COPY --from=builder /build/target/release/nebulae-server /usr/local/bin/nebulae-server
RUN chmod +x /usr/local/bin/nebulae-server

# Copy the entrypoint script.
COPY server/docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Transmission RPC + nebulae-server API + BitTorrent peer port.
EXPOSE 9091 9174 51413 51413/udp

# Data volume.
VOLUME /data

ENTRYPOINT ["tini", "--"]
CMD ["/entrypoint.sh"]
