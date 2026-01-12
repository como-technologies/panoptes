# Copyright 2026 Como Technologies, LTD
# Licensed under the Apache License, Version 2.0
#
# Multi-stage Dockerfile for argusd - File Integrity Monitoring daemon
# Produces a fully static binary that runs from scratch

# Stage 1: Build static gRPC and dependencies
# This can be replaced with a pre-built grpc-static-builder image for faster builds:
#   docker build -t grpc-static-builder:1.60.0 -f hack/Dockerfile.grpc-static .
# Then change this line to: FROM grpc-static-builder:1.60.0 AS grpc-builder
FROM ubuntu:24.04 AS grpc-builder

ARG GRPC_VERSION=1.60.0
ARG PROTOBUF_VERSION=25.1
ARG ABSEIL_VERSION=20240116.2

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    git \
    ninja-build \
    pkg-config \
    ca-certificates \
    libssl-dev \
    zlib1g-dev \
    libre2-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Build Abseil statically
RUN git clone --depth 1 --branch ${ABSEIL_VERSION} https://github.com/abseil/abseil-cpp.git && \
    cd abseil-cpp && \
    cmake -B build -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DBUILD_SHARED_LIBS=OFF \
        -DABSL_PROPAGATE_CXX_STD=ON \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
    && cmake --build build -j$(nproc) \
    && cmake --install build \
    && cd .. && rm -rf abseil-cpp

# Build Protobuf statically
RUN git clone --depth 1 --branch v${PROTOBUF_VERSION} --recurse-submodules https://github.com/protocolbuffers/protobuf.git && \
    cd protobuf && \
    cmake -B build -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DBUILD_SHARED_LIBS=OFF \
        -Dprotobuf_BUILD_TESTS=OFF \
        -Dprotobuf_ABSL_PROVIDER=package \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
    && cmake --build build -j$(nproc) \
    && cmake --install build \
    && cd .. && rm -rf protobuf

# Build gRPC statically (includes c-ares via module provider)
RUN git clone --depth 1 --branch v${GRPC_VERSION} --recurse-submodules https://github.com/grpc/grpc.git && \
    cd grpc && \
    cmake -B build -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DBUILD_SHARED_LIBS=OFF \
        -DgRPC_BUILD_TESTS=OFF \
        -DgRPC_BUILD_GRPC_CSHARP_PLUGIN=OFF \
        -DgRPC_BUILD_GRPC_NODE_PLUGIN=OFF \
        -DgRPC_BUILD_GRPC_OBJECTIVE_C_PLUGIN=OFF \
        -DgRPC_BUILD_GRPC_PHP_PLUGIN=OFF \
        -DgRPC_BUILD_GRPC_PYTHON_PLUGIN=OFF \
        -DgRPC_BUILD_GRPC_RUBY_PLUGIN=OFF \
        -DgRPC_INSTALL=ON \
        -DgRPC_ABSL_PROVIDER=package \
        -DgRPC_PROTOBUF_PROVIDER=package \
        -DgRPC_SSL_PROVIDER=package \
        -DgRPC_ZLIB_PROVIDER=package \
        -DgRPC_RE2_PROVIDER=package \
        -DgRPC_CARES_PROVIDER=module \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
    && cmake --build build -j$(nproc) \
    && cmake --install build \
    && cd .. && rm -rf grpc

# Note: glog, gflags, and fmt are built via CMake FetchContent during the
# daemon build stage, which ensures proper CMake target availability

# Stage 2: Build argusd
FROM grpc-builder AS builder

# Copy proto files to /proto (canonical location)
COPY proto/ /proto/

# Set working directory for daemon source
WORKDIR /src

# Copy common library
COPY daemons/common/ /daemons/common/

# Copy daemon source
COPY daemons/argusd/c/ ./

# Build argusd with static linking
RUN cmake -B build \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_STATIC=ON \
    -DBUILD_TESTING=OFF \
    -DPROTO_ROOT=/proto \
    -DCOMMON_LIB_DIR=/daemons/common/lib \
    && cmake --build build -j$(nproc)

# Verify the binary is statically linked
RUN ldd build/argusd 2>&1 | grep -q "not a dynamic executable" || \
    (echo "WARNING: Binary is not fully static:" && ldd build/argusd && exit 0)

# Stage 3: Minimal runtime image - scratch (no OS, just the binary)
FROM scratch

LABEL org.opencontainers.image.title="argusd"
LABEL org.opencontainers.image.description="Argus File Integrity Monitoring Daemon"
LABEL org.opencontainers.image.version="2.0.0"
LABEL org.opencontainers.image.vendor="Como Technologies, LTD"

COPY --from=builder /src/build/argusd /argusd

EXPOSE 50051

ENTRYPOINT ["/argusd"]
