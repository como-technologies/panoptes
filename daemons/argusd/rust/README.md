# argusd (Rust Implementation)

**Alternative Rust implementation of the Argus file integrity monitoring daemon**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../../LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org)

## Overview

This is an alternative Rust implementation of argusd that provides:

- Memory safety guarantees through Rust's ownership system
- Direct inotify syscalls via the `nix` crate
- Async runtime with Tokio
- gRPC server with Tonic

## Status

**Development/Alternative** - This is a scaffold implementation for benchmarking
against the primary C implementation. It is not yet production-ready.

For production use, see the [C implementation](../README.md).

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
│  │  │  (service.rs)    │  │  (service.rs)    │                       │ │
│  │  └────────┬─────────┘  └──────────────────┘                       │ │
│  └───────────┼───────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Notify Module                                 │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  notify.rs                                                   │ │ │
│  │  │  • InotifyWatcher struct                                     │ │ │
│  │  │  • nix::sys::inotify bindings                                │ │ │
│  │  │  • Async event reading with Tokio                            │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
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
├── Cargo.toml          # Rust dependencies
├── build.rs            # Proto compilation
└── src/
    ├── main.rs         # Entry point and server setup
    ├── service.rs      # gRPC service implementation
    └── notify.rs       # inotify wrapper using nix crate
```

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `nix` | 0.29 | Direct inotify syscalls |
| `tokio` | 1.x | Async runtime |
| `tonic` | 0.12 | gRPC server |
| `prost` | 0.13 | Protobuf code generation |
| `tracing` | 0.1 | Structured logging |

## Building

### Prerequisites

- Rust 1.75+ (2021 edition)
- Protobuf compiler (protoc)

### Build Commands

```bash
cd rust/

# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
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

## Implementation Details

### inotify Wrapper (notify.rs)

```rust
use nix::sys::inotify::{Inotify, AddWatchFlags, WatchDescriptor};

pub struct InotifyWatcher {
    inotify: Inotify,
    watches: HashMap<WatchDescriptor, PathBuf>,
}

impl InotifyWatcher {
    pub fn new() -> Result<Self, nix::Error> {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK)?;
        // ...
    }

    pub fn add_watch(&mut self, path: &str, flags: AddWatchFlags)
        -> Result<WatchDescriptor, nix::Error> {
        self.inotify.add_watch(path, flags)
    }

    pub async fn read_events(&self) -> Result<Vec<InotifyEvent>, Error> {
        // Non-blocking read with async polling
    }
}
```

### gRPC Service (service.rs)

```rust
#[tonic::async_trait]
impl argus_service_server::ArgusService for ArgusServiceImpl {
    async fn create_watch(
        &self,
        request: Request<CreateWatchRequest>,
    ) -> Result<Response<CreateWatchResponse>, Status> {
        // Add inotify watch for container path
    }

    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<FileEvent, Status>> + Send>>;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        // Return async stream of file events
    }
}
```

## Comparison with C Implementation

| Aspect | C Implementation | Rust Implementation |
|--------|------------------|---------------------|
| Memory safety | Manual | Guaranteed |
| Performance | Optimal | Near-optimal |
| Binary size | ~2MB | ~2MB |
| Dependencies | gRPC C++, spdlog | Tokio, Tonic |
| Build time | Fast | Slower |
| Code maturity | Production | Development |

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `ARGUSD_LISTEN_ADDR` | `0.0.0.0:50051` | gRPC listen address |
| `NODE_NAME` | `unknown` | Kubernetes node name |
| `ARGUSD_MAX_WATCHES` | `10000` | Maximum inotify watches |
| `LOG_LEVEL` | `info` | Log level |
| `RUST_LOG` | - | Rust log filter |

## Running

```bash
# Development
cargo run

# Production
./target/release/argusd

# With configuration
ARGUSD_LISTEN_ADDR=0.0.0.0:50051 \
NODE_NAME=worker-1 \
LOG_LEVEL=debug \
./target/release/argusd
```

## Docker Build

```dockerfile
FROM rust:1.75 AS builder
WORKDIR /src
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /src/target/release/argusd /argusd
ENTRYPOINT ["/argusd"]
```

## Benchmarking

To compare with the C implementation, run the benchmark suite:

```bash
cd ../../benchmarks
./scripts/run-benchmarks.sh --impl rust
./scripts/compare-impls.sh
```

See [benchmarks/README.md](../../../benchmarks/README.md) for methodology.

## Known Limitations

1. **Container runtime integration** - Simplified compared to C version
2. **Event caching** - Basic implementation
3. **Recursive watching** - Uses simpler algorithm
4. **Production testing** - Limited real-world validation

## Future Work

- [ ] Complete container runtime detection (containerd, CRI-O)
- [ ] Add event caching with deduplication
- [ ] Implement recursive directory watching
- [ ] Add comprehensive integration tests
- [ ] Performance optimization and benchmarking
- [ ] Production hardening

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../../LICENSE) for details.
