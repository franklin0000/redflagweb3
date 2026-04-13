# ─────────────────────────────────────────────────────────────
# Stage 1: Frontend (Vite/React)
# ─────────────────────────────────────────────────────────────
FROM node:20-slim AS frontend-builder
WORKDIR /app/redflag-web
COPY redflag-web/package*.json ./
RUN npm ci --silent
COPY redflag-web/ ./
RUN npm run build

# ─────────────────────────────────────────────────────────────
# Stage 2: Rust backend (with dependency caching)
# ─────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS backend-builder
WORKDIR /app

# System deps needed by aws-lc-rs + libp2p
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake clang git perl make build-essential pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifest + lockfile first for layer caching
COPY Cargo.toml Cargo.lock ./

# Create stub lib.rs/main.rs for every crate so 'cargo build' caches deps
RUN mkdir -p redflag-core/src redflag-crypto/src redflag-state/src \
             redflag-consensus/src redflag-network/src redflag-cli/src \
             redflag-vm/src \
    && echo '' | tee \
        redflag-core/src/lib.rs \
        redflag-crypto/src/lib.rs \
        redflag-state/src/lib.rs \
        redflag-consensus/src/lib.rs \
        redflag-vm/src/lib.rs \
    && echo 'fn main(){}' | tee redflag-network/src/main.rs redflag-cli/src/main.rs

# Copy each crate's Cargo.toml
COPY redflag-core/Cargo.toml        redflag-core/
COPY redflag-crypto/Cargo.toml      redflag-crypto/
COPY redflag-state/Cargo.toml       redflag-state/
COPY redflag-consensus/Cargo.toml   redflag-consensus/
COPY redflag-network/Cargo.toml     redflag-network/
COPY redflag-cli/Cargo.toml         redflag-cli/
COPY redflag-vm/Cargo.toml          redflag-vm/

# Build only deps (stubs compile in seconds, deps cached in layer)
RUN cargo build --release -p redflag-network 2>/dev/null || true
RUN find target/release -name "libredflag*" -delete 2>/dev/null || true

# Now copy real source and build
COPY redflag-core/src       redflag-core/src
COPY redflag-crypto/src     redflag-crypto/src
COPY redflag-state/src      redflag-state/src
COPY redflag-consensus/src  redflag-consensus/src
COPY redflag-network/src    redflag-network/src
COPY redflag-cli/src        redflag-cli/src
COPY redflag-vm/src         redflag-vm/src

RUN cargo build --release -p redflag-network

# ─────────────────────────────────────────────────────────────
# Stage 3: Minimal production image (~80 MB)
# ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -ms /bin/bash redflag

# Copy binaries and static assets
COPY --from=backend-builder /app/target/release/redflag-network ./redflag-node
COPY --from=frontend-builder /app/redflag-web/dist ./redflag-web/dist

# Data directory (will be mounted as volume)
RUN mkdir -p /app/data && chown -R redflag:redflag /app

USER redflag

# Health check: hit /status endpoint
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:${PORT:-8545}/status || exit 1

EXPOSE 8545 9000

# Environment defaults (overridable via docker-compose / -e flags)
ENV PORT=8545 \
    P2P_PORT=9000 \
    DATA_DIR=/app/data \
    RUST_LOG=info

CMD ["./redflag-node"]
