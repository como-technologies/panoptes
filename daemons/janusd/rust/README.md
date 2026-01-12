# janusd (Rust Implementation)

**Production-ready Rust implementation of the Janus file access auditing daemon**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../../LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82+-orange.svg)](https://www.rust-lang.org)

## Overview

This is a complete Rust implementation of janusd that provides:

- Memory safety guarantees through Rust's ownership system
- Direct fanotify syscalls via the `nix` crate
- Async runtime with Tokio
- gRPC server with Tonic
- Container runtime integration (containerd, CRI-O)
- Policy evaluation with LRU caching
- Event deduplication (100ms window)
- Kernel audit logging via NETLINK_AUDIT

## Status

**Production-Ready** - Feature-complete implementation matching the C daemon.
Fully tested with 63 unit tests and 21 integration tests.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      janusd (Rust) Architecture                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Tonic gRPC Server                               │ │
│  │  ┌──────────────────┐  ┌──────────────────┐                       │ │
│  │  │  JanusService    │  │  HealthService   │                       │ │
│  │  │  (service.rs)    │  │  (tonic-health)  │                       │ │
│  │  └────────┬─────────┘  └──────────────────┘                       │ │
│  └───────────┼───────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Guard Module                                  │ │
│  │  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────────┐  │ │
│  │  │ FanotifyGuard   │ │ PolicyEvaluator │ │ DedupeCache         │  │ │
│  │  │ (nix fanotify)  │ │ (LRU cached)    │ │ (100ms window)      │  │ │
│  │  └─────────────────┘ └─────────────────┘ └─────────────────────┘  │ │
│  │  ┌─────────────────┐ ┌─────────────────────────────────────────┐  │ │
│  │  │ AuditLogger     │ │ ContainerRuntime (containerd/CRI-O)     │  │ │
│  │  │ (NETLINK_AUDIT) │ └─────────────────────────────────────────┘  │ │
│  │  └─────────────────┘                                              │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Linux Kernel                                  │ │
│  │  fanotify_init() → fanotify_mark() → read() → write(response)     │ │
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
│   ├── guard.rs         # fanotify wrapper, Guard struct
│   ├── policy.rs        # PolicyEvaluator with LRU caching
│   ├── dedupe.rs        # Event deduplication cache
│   ├── audit.rs         # Kernel audit logging (NETLINK_AUDIT)
│   └── metrics.rs       # Atomic metrics collection
├── tests/
│   └── integration_tests.rs  # 21 integration tests
└── benches/
    └── policy_evaluation.rs  # 6 benchmark groups
```

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| CreateGuard RPC | ✅ | Create fanotify guards for container paths |
| DestroyGuard RPC | ✅ | Clean up guards and resources |
| GetGuardState RPC | ✅ | Streaming guard state updates |
| StreamAccessEvents RPC | ✅ | Real-time access events with filtering |
| GetMetrics RPC | ✅ | Daemon-level metrics |
| Policy evaluation | ✅ | Deny/allow patterns with LRU caching |
| Event deduplication | ✅ | 100ms window, 64-entry circular buffer |
| Kernel audit | ✅ | NETLINK_AUDIT integration |
| Container runtime | ✅ | Auto-detection (containerd, CRI-O) |
| Health checks | ✅ | gRPC health service (tonic-health) |

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `nix` | 0.29 | Direct fanotify syscalls |
| `tokio` | 1.x | Async runtime |
| `tonic` | 0.12 | gRPC server |
| `tonic-health` | 0.12 | gRPC health checks |
| `prost` | 0.13 | Protobuf code generation |
| `tracing` | 0.1 | Structured JSON logging |
| `glob` | 0.3 | Pattern matching for policies |
| `panoptes-common` | 0.1 | Shared container runtime library |

## Building

### Prerequisites

- Rust 1.82+ (for dependency compatibility)
- Protobuf compiler (protoc)
- Linux kernel 5.x+ (for fanotify features)

### Build Commands

```bash
cd daemons/janusd/rust/

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
| `--listen-addr` | `JANUSD_LISTEN_ADDR` | `0.0.0.0:50052` | gRPC listen address |
| `--port` | `JANUSD_PORT` | - | Port override (C daemon compatibility) |
| `--node-name` | `NODE_NAME` | `unknown` | Kubernetes node name |
| `--max-guards` | `JANUSD_MAX_GUARDS` | `1000` | Maximum concurrent guards |
| `--log-level` | `LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |

## Running

```bash
# Development
cargo run

# Production
./target/release/janusd

# With configuration
./target/release/janusd --port=50052 --node-name=worker-1 --log-level=debug

# Using environment variables
JANUSD_PORT=50052 NODE_NAME=worker-1 LOG_LEVEL=debug ./target/release/janusd
```

## Docker Build

The Dockerfile is at `daemons/janusd/Dockerfile.rust`:

```bash
# Build from repo root (context needs proto/)
docker build -t janusd-rust:latest -f daemons/janusd/Dockerfile.rust .

# Run (requires CAP_SYS_ADMIN for fanotify)
docker run --privileged -p 50052:50052 janusd-rust:latest
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_tests

# Specific test
cargo test test_deny_pattern_blocks_access
```

### Test Coverage

| Module | Unit Tests | Integration Tests |
|--------|------------|-------------------|
| guard.rs | 20 | 5 |
| policy.rs | 17 | 5 |
| dedupe.rs | 12 | 7 |
| audit.rs | 9 | 4 |
| service.rs | 5 | 0 |
| **Total** | **63** | **21** |

## Benchmarks

```bash
cargo bench
```

| Benchmark | Description |
|-----------|-------------|
| policy_evaluation_no_cache | Policy evaluation without LRU cache |
| policy_evaluation_with_cache | Policy evaluation with LRU cache |
| deduplication | Event deduplication performance |
| glob_patterns | Pattern compilation and matching |
| path_operations | Path string operations |
| lru_cache | LRU cache operations at various sizes |

## Comparison with C Implementation

| Aspect | C Implementation | Rust Implementation |
|--------|------------------|---------------------|
| Memory safety | Manual | Guaranteed |
| Performance | Optimal | Near-optimal |
| Binary size | ~2MB | ~2MB |
| Image size | ~3MB (scratch) | ~50MB (debian-slim) |
| Dependencies | gRPC C++, spdlog, libaudit | Tokio, Tonic |
| Build time | Fast (with cache) | Slower |
| Audit integration | libaudit | NETLINK_AUDIT direct |
| Testing | GoogleTest | Rust built-in |

## Kernel Audit Integration

The daemon logs denied access events to the kernel audit log via NETLINK_AUDIT:

```
type=JANUS_DENY msg=audit(1234567890.123:456): path="/etc/shadow" pid=1234 comm="cat" exe="/usr/bin/cat"
```

Requires `CAP_AUDIT_WRITE` capability. Falls back to `NullAuditLogger` if unavailable.

## Kubernetes Deployment

The daemon is deployed as a DaemonSet. See `hack/janusd-daemonset.yaml`:

```yaml
args:
- --port=50052  # Works with both C and Rust
```

Both implementations accept the same `--port` argument for compatibility.

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../../LICENSE) for details.
