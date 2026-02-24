# Copyright 2026 Como Technologies, LTD
# Licensed under the Apache License, Version 2.0
#
# Multi-stage build for minimal watcher-wait init container image
#
# Usage:
#   docker build -t panoptes/watcher-wait:latest -f tools/watcher-wait/Dockerfile .
#
# Size: ~5MB (static musl binary on scratch)

# ============================================================================
# Stage 1: Chef planner (generates recipe.json for dependency caching)
# ============================================================================
FROM rust:1.93-alpine AS planner

COPY --from=lukemathwalker/cargo-chef:latest-rust-1.93.1-alpine /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy only what's needed for dependency resolution
COPY proto/ ./proto/
COPY tools/watcher-wait/Cargo.toml tools/watcher-wait/Cargo.lock* ./tools/watcher-wait/
COPY tools/watcher-wait/build.rs ./tools/watcher-wait/

# Create dummy src to satisfy cargo
RUN mkdir -p tools/watcher-wait/src && \
    echo "fn main() {}" > tools/watcher-wait/src/main.rs

WORKDIR /build/tools/watcher-wait
RUN cargo chef prepare --recipe-path recipe.json

# ============================================================================
# Stage 2: Chef cook (builds and caches dependencies)
# ============================================================================
FROM rust:1.93-alpine AS cacher

COPY --from=lukemathwalker/cargo-chef:latest-rust-1.93.1-alpine /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
RUN apk add --no-cache musl-dev protobuf-dev

WORKDIR /build

# Copy proto files (needed for build.rs)
COPY proto/ ./proto/

# Copy recipe from planner
COPY --from=planner /build/tools/watcher-wait/recipe.json ./tools/watcher-wait/recipe.json

# Copy build.rs for proto generation
COPY tools/watcher-wait/build.rs ./tools/watcher-wait/

# Create dummy src (cargo chef cook needs valid project structure)
RUN mkdir -p tools/watcher-wait/src && \
    echo "fn main() {}" > tools/watcher-wait/src/main.rs

# Cook dependencies (this layer is cached if dependencies don't change)
RUN cd tools/watcher-wait && cargo chef cook --release --recipe-path recipe.json

# ============================================================================
# Stage 3: Build
# ============================================================================
FROM rust:1.93-alpine AS builder

RUN apk add --no-cache musl-dev protobuf-dev

WORKDIR /build

# Copy proto files
COPY proto/ ./proto/

# Copy cached dependencies from cacher stage
COPY --from=cacher /build/tools/watcher-wait/target ./tools/watcher-wait/target
COPY --from=cacher /usr/local/cargo/registry /usr/local/cargo/registry

# Copy watcher-wait source
COPY tools/watcher-wait/ ./tools/watcher-wait/

WORKDIR /build/tools/watcher-wait

# Build release binary - deps are cached
RUN cargo build --release

# ============================================================================
# Stage 4: Minimal runtime (FROM scratch)
# ============================================================================
FROM scratch

# Copy CA certificates for TLS (if needed)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/tools/watcher-wait/target/release/watcher-wait /watcher-wait

# Run as non-root (uid 1000)
USER 1000

ENTRYPOINT ["/watcher-wait"]

# Labels
LABEL org.opencontainers.image.title="Panoptes Watcher Wait" \
      org.opencontainers.image.description="Init container that waits for ArgusWatcher readiness" \
      org.opencontainers.image.vendor="Como Technologies" \
      org.opencontainers.image.source="https://github.com/como-technologies/panoptes"
