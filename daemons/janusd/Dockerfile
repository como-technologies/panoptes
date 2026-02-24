# Copyright 2026 Como Technologies, LTD
# Licensed under the Apache License, Version 2.0
#
# Unified Dockerfile for janusd (Rust) - File Access Auditing daemon
# Uses multi-stage build with musl for static linking and scratch base
#
# Build Args:
#   FEATURES=""       -> Slim build (~5-6 MB): traditional fanotify mode only
#   FEATURES="ebpf"   -> Full build (~6-8 MB): eBPF + fanotify with auto-detection
#
# Usage:
#   docker build -f daemons/janusd/Dockerfile --build-arg FEATURES= -t janusd:slim .
#   docker build -f daemons/janusd/Dockerfile --build-arg FEATURES=ebpf -t janusd:latest .

# ============================================================================
# Stage 1: Chef planner (generates recipe.json for dependency caching)
# ============================================================================
FROM rust:1.93-alpine AS planner

COPY --from=lukemathwalker/cargo-chef:latest-rust-1.93.1-alpine /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy only what's needed for dependency resolution
COPY proto/ ./proto/
COPY daemons/common/ ./daemons/common/
COPY daemons/janusd/Cargo.toml daemons/janusd/Cargo.lock ./daemons/janusd/
COPY daemons/janusd/build.rs ./daemons/janusd/

# Create dummy src to satisfy cargo
RUN mkdir -p daemons/janusd/src daemons/janusd/benches && \
    echo "fn main() {}" > daemons/janusd/src/main.rs && \
    echo "fn main() {}" > daemons/janusd/benches/policy_evaluation.rs

WORKDIR /build/daemons/janusd
RUN cargo chef prepare --recipe-path recipe.json

# ============================================================================
# Stage 2: Chef cook (builds and caches dependencies)
# ============================================================================
FROM rust:1.93-alpine AS cacher

ARG TARGETARCH

COPY --from=lukemathwalker/cargo-chef:latest-rust-1.93.1-alpine /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
RUN apk add --no-cache musl-dev protobuf-dev && \
    if [ "$TARGETARCH" = "arm64" ]; then \
      rustup target add aarch64-unknown-linux-musl; \
    else \
      rustup target add x86_64-unknown-linux-musl; \
    fi

WORKDIR /build

# Copy proto files (needed for build.rs)
COPY proto/ ./proto/

# Copy common crate
COPY daemons/common/ ./daemons/common/

# Copy recipe from planner
COPY --from=planner /build/daemons/janusd/recipe.json ./daemons/janusd/recipe.json

# Copy build.rs for proto generation
COPY daemons/janusd/build.rs ./daemons/janusd/

# Create dummy src and bench files (cargo chef cook needs valid project structure)
RUN mkdir -p daemons/janusd/src daemons/janusd/benches && \
    echo "fn main() {}" > daemons/janusd/src/main.rs && \
    echo "fn main() {}" > daemons/janusd/benches/policy_evaluation.rs

ARG FEATURES=""

# Cook dependencies (this layer is cached if dependencies don't change)
RUN RUST_TARGET=$(if [ "$TARGETARCH" = "arm64" ]; then echo "aarch64-unknown-linux-musl"; else echo "x86_64-unknown-linux-musl"; fi) && \
    cd daemons/janusd && \
    if [ -n "$FEATURES" ]; then \
      cargo chef cook --release --target $RUST_TARGET --features "$FEATURES" --recipe-path recipe.json; \
    else \
      cargo chef cook --release --target $RUST_TARGET --recipe-path recipe.json; \
    fi

# ============================================================================
# Stage 3: eBPF bytecode builder (only used when FEATURES contains "ebpf")
# ============================================================================
FROM rust:1.93-bookworm AS ebpf-builder

ARG FEATURES=""

# Only install eBPF tools if building with ebpf feature
RUN if echo "$FEATURES" | grep -q "ebpf"; then \
      apt-get update && apt-get install -y --no-install-recommends \
        llvm \
        clang \
        libclang-dev \
        linux-headers-generic \
      && rm -rf /var/lib/apt/lists/* \
      && cargo install bpf-linker --locked \
      && rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu; \
    fi

WORKDIR /build

# Copy source for eBPF build
COPY daemons/common/ ./daemons/common/
COPY daemons/janusd/ ./daemons/janusd/

# Build eBPF kernel program (only if ebpf feature requested)
RUN if echo "$FEATURES" | grep -q "ebpf"; then \
      cd daemons/janusd/ebpf && \
      cargo +nightly build --release --target bpfel-unknown-none -Z build-std=core; \
    else \
      mkdir -p daemons/janusd/target/bpf/bpfel-unknown-none/release && \
      touch daemons/janusd/target/bpf/bpfel-unknown-none/release/.placeholder; \
    fi

# ============================================================================
# Stage 4: Userspace binary builder (uses cached dependencies)
# ============================================================================
FROM rust:1.93-alpine AS builder

# Docker provides TARGETARCH (amd64, arm64)
ARG TARGETARCH

# Install build dependencies and add appropriate musl target
RUN apk add --no-cache \
    musl-dev \
    protobuf-dev \
    && if [ "$TARGETARCH" = "arm64" ]; then \
         rustup target add aarch64-unknown-linux-musl; \
       else \
         rustup target add x86_64-unknown-linux-musl; \
       fi

WORKDIR /build

# Copy proto files
COPY proto/ ./proto/

# Copy shared libraries
COPY daemons/common/ ./daemons/common/

# Copy cached dependencies from cacher stage
COPY --from=cacher /build/daemons/janusd/target ./daemons/janusd/target
COPY --from=cacher /usr/local/cargo/registry /usr/local/cargo/registry

# Copy daemon source
COPY daemons/janusd/ ./daemons/janusd/

ARG FEATURES=""

# Build static binary with musl (architecture-aware) - deps are cached
RUN RUST_TARGET=$(if [ "$TARGETARCH" = "arm64" ]; then echo "aarch64-unknown-linux-musl"; else echo "x86_64-unknown-linux-musl"; fi) && \
    cd daemons/janusd && \
    if [ -n "$FEATURES" ]; then \
      cargo build --release --target $RUST_TARGET --features "$FEATURES"; \
    else \
      cargo build --release --target $RUST_TARGET; \
    fi

# ============================================================================
# Stage 5: Minimal runtime (FROM scratch)
# ============================================================================
FROM scratch

ARG TARGETARCH

LABEL org.opencontainers.image.title="janusd"
LABEL org.opencontainers.image.description="Janus File Access Auditing Daemon (Rust)"
LABEL org.opencontainers.image.version="2.0.0"
LABEL org.opencontainers.image.vendor="Como Technologies, LTD"

# Copy CA certificates for TLS (from alpine builder)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy static binary (use glob pattern to match either arch)
COPY --from=builder /build/daemons/janusd/target/*-unknown-linux-musl/release/janusd /janusd

# Copy eBPF bytecode (only exists in ebpf builds, COPY will fail gracefully)
COPY --from=ebpf-builder /build/daemons/janusd/target/bpf/bpfel-unknown-none/release/janusd-ebpf* /ebpf/

# Runtime auto-detection: eBPF if kernel supports it, otherwise fanotify
# Override with JANUSD_MODE=traditional or JANUSD_MODE=ebpf
ENV JANUSD_MODE=auto

EXPOSE 50052

ENTRYPOINT ["/janusd"]
