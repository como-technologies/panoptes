# argusd (Rust Implementation)

**Production-ready Rust implementation of the Argus file integrity monitoring daemon**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../../LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82+-orange.svg)](https://www.rust-lang.org)

## Overview

This is a complete Rust implementation of argusd that provides:

- Memory safety guarantees through Rust's ownership system
- Direct inotify syscalls via the `nix` crate
- Async runtime with Tokio
- gRPC server with Tonic
- Container runtime integration (containerd, CRI-O)
- Move event pairing with cookie-based tracking
- Glob pattern filtering for ignore paths
- Metrics collection with atomic operations

## Status

**Production-Ready** - Feature-complete implementation matching the C daemon.
Fully tested with 54 unit tests and 12 integration tests.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      argusd (Rust) Architecture                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Tonic gRPC Server                               │ │
│  │  ┌──────────────────┐  ┌──────────────────┐                       │ │
│  │  │  ArgusService    │  │  HealthService   │                       │ │
│  │  │  (service.rs)    │  │  (tonic-health)  │                       │ │
│  │  └────────┬─────────┘  └──────────────────┘                       │ │
│  └───────────┼───────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Watcher Module                                │ │
│  │  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────────┐  │ │
│  │  │ InotifyInstance │ │ MovePairTracker │ │ ContainerRuntime    │  │ │
│  │  │ (nix inotify)   │ │ (cookie-based)  │ │ (containerd/CRI-O)  │  │ │
│  │  └─────────────────┘ └─────────────────┘ └─────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Linux Kernel                                  │ │
│  │  inotify_init1() → inotify_add_watch() → read()                   │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

## Project Structure

```
rust/
├── Cargo.toml           # Rust dependencies
├── build.rs             # Proto compilation (tonic-build)
├── src/
│   ├── main.rs          # Entry point and CLI configuration
│   ├── service.rs       # gRPC service implementation
│   ├── notify.rs        # inotify wrapper, Watcher, MovePairTracker
│   └── metrics.rs       # Atomic metrics collection
├── tests/
│   └── integration_tests.rs  # 12 integration tests
└── benches/
    └── event_processing.rs   # 5 benchmark groups
```

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| CreateWatch RPC | ✅ | Create inotify watches for container paths |
| DestroyWatch RPC | ✅ | Clean up watches and resources |
| GetWatchState RPC | ✅ | Streaming watch state updates |
| StreamEvents RPC | ✅ | Real-time file events with filtering |
| GetMetrics RPC | ✅ | Daemon-level metrics |
| Move event pairing | ✅ | Cookie-based pairing with 2ms timeout |
| Cache consistency | ✅ | Stale watch cleanup on reconnect |
| Overflow recovery | ✅ | Reinit with config preservation |
| Container runtime | ✅ | Auto-detection (containerd, CRI-O) |
| Glob filtering | ✅ | Ignore patterns like `*.tmp`, `node_modules/**` |
| Health checks | ✅ | gRPC health service (tonic-health) |

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `nix` | 0.29 | Direct inotify syscalls |
| `tokio` | 1.x | Async runtime |
| `tonic` | 0.12 | gRPC server |
| `tonic-health` | 0.12 | gRPC health checks |
| `prost` | 0.13 | Protobuf code generation |
| `tracing` | 0.1 | Structured JSON logging |
| `glob` | 0.3 | Pattern matching for ignore paths |
| `panoptes-common` | 0.1 | Shared container runtime library |

## Building

### Prerequisites

- Rust 1.82+ (for dependency compatibility)
- Protobuf compiler (protoc)

### Build Commands

```bash
cd daemons/argusd/rust/

# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Format and lint
cargo fmt --check && cargo clippy --all-targets
```

### Build Profile

The `Cargo.toml` includes an optimized release profile:

```toml
[profile.release]
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization
strip = true         # Strip symbols
panic = "abort"      # No unwinding
```

## Configuration

| CLI Argument | Environment Variable | Default | Description |
|--------------|---------------------|---------|-------------|
| `--listen-addr` | `ARGUSD_LISTEN_ADDR` | `0.0.0.0:50051` | gRPC listen address |
| `--port` | `ARGUSD_PORT` | - | Port override (C daemon compatibility) |
| `--node-name` | `NODE_NAME` | `unknown` | Kubernetes node name |
| `--max-watches` | `ARGUSD_MAX_WATCHES` | `10000` | Maximum inotify watches |
| `--log-level` | `LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |

## Running

```bash
# Development
cargo run

# Production
./target/release/argusd

# With configuration
./target/release/argusd --port=50051 --node-name=worker-1 --log-level=debug

# Using environment variables
ARGUSD_PORT=50051 NODE_NAME=worker-1 LOG_LEVEL=debug ./target/release/argusd
```

## Docker Build

The Dockerfile is at `daemons/argusd/Dockerfile.rust`:

```bash
# Build from repo root (context needs proto/)
docker build -t argusd-rust:latest -f daemons/argusd/Dockerfile.rust .

# Run
docker run --privileged -p 50051:50051 argusd-rust:latest
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests (requires inotify)
cargo test --test integration_tests

# Specific test
cargo test test_file_creation_detection
```

### Test Coverage

| Module | Unit Tests | Integration Tests |
|--------|------------|-------------------|
| notify.rs | 35 | 6 |
| service.rs | 12 | 3 |
| metrics.rs | 7 | 1 |
| **Total** | **54** | **12** |

## Benchmarks

```bash
cargo bench
```

| Benchmark | Description |
|-----------|-------------|
| glob_matching | Pattern matching performance |
| event_filtering | Vec vs HashSet contains |
| move_pair_tracking | Cookie-based move pairing |
| metrics_collection | Atomic counter operations |
| path_processing | Path manipulation operations |

## Comparison with C Implementation

| Aspect | C Implementation | Rust Implementation |
|--------|------------------|---------------------|
| Memory safety | Manual | Guaranteed |
| Performance | Optimal | Near-optimal |
| Binary size | ~2MB | ~2MB |
| Image size | ~3MB (scratch) | ~50MB (debian-slim) |
| Dependencies | gRPC C++, spdlog | Tokio, Tonic |
| Build time | Fast (with cache) | Slower |
| Testing | GoogleTest | Rust built-in |

## Kubernetes Deployment

The daemon is deployed as a DaemonSet. See `hack/argusd-daemonset.yaml`:

```yaml
args:
- --port=50051  # Works with both C and Rust
```

Both implementations accept the same `--port` argument for compatibility.

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../../LICENSE) for details.
