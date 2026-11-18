# syntax=docker/dockerfile:1
# Nebulae Server with Transmission 4.1.3 Engine
#
# Multi-stage build:
#   Stage 1: Build nebulae-server (Rust)
#   Stage 2: Build transmission-daemon 4.1.3 (C++ / CMake / Ninja) with ADC identity override patch
#   Stage 3: Minimal Debian Bookworm runtime image
#
# Build context is the repository root:
#   docker build -f server/docker/Dockerfile --build-arg BUILD_ID=$(git rev-parse HEAD) -t nebulae-server .

# ── Stage 1: Build nebulae-server ──────────────────────────────────────────
FROM rust:bookworm AS server-builder

WORKDIR /build
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src ./src
RUN cargo build --release --locked 2>/dev/null || cargo build --release

# ── Stage 2: Build Transmission 4.1.3 ──────────────────────────────────────
FROM debian:bookworm-slim AS transmission-builder

ARG TRANSMISSION_VERSION=4.1.3
ARG TRANSMISSION_SHA256=ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    cmake \
    ninja-build \
    pkg-config \
    build-essential \
    libssl-dev \
    libcurl4-openssl-dev \
    libminiupnpc-dev \
    libnatpmp-dev \
    libdeflate-dev \
    zlib1g-dev \
    libpsl-dev \
    patch \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
RUN curl -fsSL "https://github.com/transmission/transmission/releases/download/${TRANSMISSION_VERSION}/transmission-${TRANSMISSION_VERSION}.tar.xz" -o transmission.tar.xz \
    && echo "${TRANSMISSION_SHA256}  transmission.tar.xz" | sha256sum -c - \
    && tar -xJf transmission.tar.xz

COPY server/docker/transmission-4.1.3-identity-override.patch /tmp/identity.patch
RUN patch -d "transmission-${TRANSMISSION_VERSION}" -p1 < /tmp/identity.patch

WORKDIR /src/transmission-${TRANSMISSION_VERSION}/build
RUN cmake .. \
    -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr/local \
    -DENABLE_DAEMON=ON \
    -DENABLE_GTK=OFF \
    -DENABLE_QT=OFF \
    -DENABLE_UTILS=OFF \
    -DENABLE_MAC=OFF \
    -DENABLE_TESTS=OFF \
    -DENABLE_CLI=OFF \
    -DRUN_CLANG_TIDY=OFF \
    -DINSTALL_DOC=OFF \
    -DWITH_CRYPTO=openssl \
    -DWITH_MINIUPNPC=ON \
    -DWITH_NATPMP=ON \
    && ninja transmission-daemon

# ── Stage 3: Runtime image ─────────────────────────────────────────────────
FROM debian:bookworm-slim

ARG PUID=1000
ARG PGID=1000
ARG NEBULAE_VERSION=0.2.8
ARG TRANSMISSION_VERSION=4.1.3
ARG BUILD_ID=unknown

LABEL org.opencontainers.image.title="Nebulae Server" \
      org.opencontainers.image.description="Headless Nebulae bridge with Transmission 4.1.3" \
      org.opencontainers.image.source="https://github.com/koppot/Nebulae-torrent_client" \
      org.opencontainers.image.version="${NEBULAE_VERSION}" \
      org.opencontainers.image.revision="${BUILD_ID}" \
      io.nebulae.transmission.version="${TRANSMISSION_VERSION}" \
      io.nebulae.transmission.patch="transmission-4.1.3-identity-override"

ENV NEBULAE_BUILD_ID=${BUILD_ID}

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    tini \
    procps \
    libssl3 \
    libcurl4 \
    libminiupnpc17 \
    libnatpmp1 \
    libdeflate0 \
    libpsl5 \
    && rm -rf /var/lib/apt/lists/*

# Create user and data directories
RUN groupadd -g ${PGID} nebulae && \
    useradd -u ${PUID} -g nebulae -m -s /bin/bash nebulae && \
    mkdir -p /data/downloads /data/watch /data/config && \
    chown -R nebulae:nebulae /data

# Copy built binaries
COPY --from=server-builder /build/target/release/nebulae-server /usr/local/bin/nebulae-server
COPY --from=transmission-builder /src/transmission-4.1.3/build/daemon/transmission-daemon /usr/local/bin/transmission-daemon
COPY server/docker/entrypoint.sh /entrypoint.sh

RUN chmod +x /usr/local/bin/nebulae-server /usr/local/bin/transmission-daemon /entrypoint.sh

# Expose ports: REST API (9174), BitTorrent peer port (51413 TCP/UDP)
EXPOSE 9174 51413 51413/udp

VOLUME ["/data/downloads", "/data/config", "/data/watch"]

ENTRYPOINT ["tini", "--"]
CMD ["/entrypoint.sh"]
