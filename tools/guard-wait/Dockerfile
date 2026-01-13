# Copyright 2026 Como Technologies, LTD
# Licensed under the Apache License, Version 2.0
#
# Multi-stage build for minimal guard-wait init container image
#
# Usage:
#   docker build -t panoptes/guard-wait:latest -f tools/guard-wait/Dockerfile .
#
# Size: ~5MB (static musl binary on scratch)

# Build stage
FROM rust:1.82-alpine AS builder

# Install musl-dev for static linking and protoc
RUN apk add --no-cache musl-dev protobuf-dev

# Create non-root user for build
RUN adduser -D -u 1000 builder
USER builder
WORKDIR /home/builder

# Copy proto files first (for caching)
COPY --chown=builder:builder proto/ /home/builder/proto/

# Copy guard-wait source
COPY --chown=builder:builder tools/guard-wait/ /home/builder/tools/guard-wait/

WORKDIR /home/builder/tools/guard-wait

# Build release binary with static linking
RUN cargo build --release

# Runtime stage - minimal scratch image
FROM scratch

# Copy CA certificates for TLS (if needed)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /home/builder/tools/guard-wait/target/release/guard-wait /guard-wait

# Run as non-root (uid 1000)
USER 1000

ENTRYPOINT ["/guard-wait"]

# Labels
LABEL org.opencontainers.image.title="Panoptes Guard Wait" \
      org.opencontainers.image.description="Init container that waits for JanusGuard readiness" \
      org.opencontainers.image.vendor="Como Technologies" \
      org.opencontainers.image.source="https://github.com/como-technologies/panoptes"
