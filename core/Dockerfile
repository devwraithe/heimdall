# ── Stage 1: Build ──
FROM rust:1.87-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev protobuf-compiler libzstd-dev build-essential ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency compilation
COPY core/Cargo.toml core/Cargo.lock* ./
COPY core/observation/Cargo.toml observation/Cargo.toml
COPY core/intelligence/Cargo.toml intelligence/Cargo.toml
COPY core/submission/Cargo.toml submission/Cargo.toml
COPY core/tracking/Cargo.toml tracking/Cargo.toml
COPY core/state/Cargo.toml state/Cargo.toml
COPY core/shared/Cargo.toml shared/Cargo.toml

# Create dummy sources for dependency caching
RUN mkdir -p observation/src intelligence/src submission/src tracking/src state/src shared/src && \
    echo "fn main() {}" > observation/src/main.rs && \
    echo "" > intelligence/src/lib.rs && \
    echo "" > submission/src/lib.rs && \
    echo "" > tracking/src/lib.rs && \
    echo "" > state/src/lib.rs && \
    echo "" > shared/src/lib.rs

RUN cargo build --release 2>/dev/null || true

# Copy real sources
COPY core/ .
COPY proto/ /app/proto/

RUN cargo build --release

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 libzstd1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/observation /app/heimdall

RUN mkdir -p /app/shared

CMD ["/app/heimdall"]
