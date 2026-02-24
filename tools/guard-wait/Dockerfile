# Copyright 2026 Como Technologies, LTD
# Licensed under the Apache License, Version 2.0
#
# Multi-stage build for minimal guard-wait init container image
#
# Usage:
#   docker build -t panoptes/guard-wait:latest -f tools/guard-wait/Dockerfile .
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
COPY tools/guard-wait/Cargo.toml tools/guard-wait/Cargo.lock* ./tools/guard-wait/
COPY tools/guard-wait/build.rs ./tools/guard-wait/

# Create dummy src to satisfy cargo
RUN mkdir -p tools/guard-wait/src && \
    echo "fn main() {}" > tools/guard-wait/src/main.rs

WORKDIR /build/tools/guard-wait
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
COPY --from=planner /build/tools/guard-wait/recipe.json ./tools/guard-wait/recipe.json

# Copy build.rs for proto generation
COPY tools/guard-wait/build.rs ./tools/guard-wait/

# Create dummy src (cargo chef cook needs valid project structure)
RUN mkdir -p tools/guard-wait/src && \
    echo "fn main() {}" > tools/guard-wait/src/main.rs

# Cook dependencies (this layer is cached if dependencies don't change)
RUN cd tools/guard-wait && cargo chef cook --release --recipe-path recipe.json

# ============================================================================
# Stage 3: Build
# ============================================================================
FROM rust:1.93-alpine AS builder

RUN apk add --no-cache musl-dev protobuf-dev

WORKDIR /build

# Copy proto files
COPY proto/ ./proto/

# Copy cached dependencies from cacher stage
COPY --from=cacher /build/tools/guard-wait/target ./tools/guard-wait/target
COPY --from=cacher /usr/local/cargo/registry /usr/local/cargo/registry

# Copy guard-wait source
COPY tools/guard-wait/ ./tools/guard-wait/

WORKDIR /build/tools/guard-wait

# Build release binary - deps are cached
RUN cargo build --release

# ============================================================================
# Stage 4: Minimal runtime (FROM scratch)
# ============================================================================
FROM scratch

# Copy CA certificates for TLS (if needed)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/tools/guard-wait/target/release/guard-wait /guard-wait

# Run as non-root (uid 1000)
USER 1000

ENTRYPOINT ["/guard-wait"]

# Labels
LABEL org.opencontainers.image.title="Panoptes Guard Wait" \
      org.opencontainers.image.description="Init container that waits for JanusGuard readiness" \
      org.opencontainers.image.vendor="Como Technologies" \
      org.opencontainers.image.source="https://github.com/como-technologies/panoptes"
