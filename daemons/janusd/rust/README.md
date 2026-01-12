# janusd (Rust Implementation)

**Alternative Rust implementation of the Janus file access auditing daemon**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../../LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org)

## Overview

This is an alternative Rust implementation of janusd that provides:

- Memory safety guarantees through Rust's ownership system
- Direct fanotify syscalls via the `nix` crate
- Async runtime with Tokio
- gRPC server with Tonic

## Status

**Development/Alternative** - This is a scaffold implementation for benchmarking
against the primary C implementation. It is not yet production-ready.

For production use, see the [C implementation](../README.md).

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
│  │  │  (service.rs)    │  │  (service.rs)    │                       │ │
│  │  └────────┬─────────┘  └──────────────────┘                       │ │
│  └───────────┼───────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Guard Module                                  │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  guard.rs                                                    │ │ │
│  │  │  • Guard struct with fanotify                                │ │ │
│  │  │  • nix::sys::fanotify bindings                               │ │ │
│  │  │  • Policy evaluation (allow/deny patterns)                   │ │ │
│  │  │  • FanotifyResponse for permission events                    │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
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
├── Cargo.toml          # Rust dependencies
├── build.rs            # Proto compilation
└── src/
    ├── main.rs         # Entry point and server setup
    ├── service.rs      # gRPC service implementation
    └── guard.rs        # fanotify wrapper using nix crate
```

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `nix` | 0.29 | Direct fanotify syscalls |
| `tokio` | 1.x | Async runtime |
| `tonic` | 0.12 | gRPC server |
| `prost` | 0.13 | Protobuf code generation |
| `tracing` | 0.1 | Structured logging |
| `glob` | 0.3 | Glob pattern matching |

## Building

### Prerequisites

- Rust 1.75+ (2021 edition)
- Protobuf compiler (protoc)
- Linux kernel 5.x+ (for fanotify features)

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

### fanotify Wrapper (guard.rs)

```rust
use nix::sys::fanotify::{
    Fanotify, FanotifyResponse, InitFlags, MarkFlags, MaskFlags, Response,
};

pub struct Guard {
    fanotify: Fanotify,
    config: GuardConfig,
    running: Arc<AtomicBool>,
}

impl Guard {
    pub fn new(config: GuardConfig) -> Result<Self, GuardError> {
        let init_flags = if config.enforce {
            InitFlags::FAN_CLASS_CONTENT | InitFlags::FAN_CLOEXEC
        } else {
            InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC
        };

        let fanotify = Fanotify::init(init_flags, event_flags)?;
        // ...
    }

    pub fn add_mount(&self, path: &Path) -> Result<(), GuardError> {
        let mark_flags = MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT;
        self.fanotify.mark(mark_flags, mask, None, Some(path))?;
        Ok(())
    }

    fn check_access(&self, path: &str) -> AccessResponse {
        // Check deny patterns first
        for pattern in &self.config.deny_patterns {
            if glob::Pattern::new(pattern)
                .map(|p| p.matches(path))
                .unwrap_or(false)
            {
                return AccessResponse::Deny;
            }
        }
        // Check allow patterns...
    }
}
```

### gRPC Service (service.rs)

```rust
#[tonic::async_trait]
impl janus_service_server::JanusService for JanusServiceImpl {
    async fn create_guard(
        &self,
        request: Request<CreateGuardRequest>,
    ) -> Result<Response<CreateGuardResponse>, Status> {
        // Create fanotify guard for container
    }

    type StreamEventsStream = Pin<
        Box<dyn Stream<Item = Result<AccessEvent, Status>> + Send>
    >;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        // Return async stream of access events
    }
}
```

## Comparison with C Implementation

| Aspect | C Implementation | Rust Implementation |
|--------|------------------|---------------------|
| Memory safety | Manual | Guaranteed |
| Performance | Optimal | Near-optimal |
| Binary size | ~2MB | ~2MB |
| Dependencies | gRPC C++, spdlog, libaudit | Tokio, Tonic |
| Build time | Fast | Slower |
| Audit integration | Full | Basic |
| Code maturity | Production | Development |

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `JANUSD_LISTEN_ADDR` | `0.0.0.0:50052` | gRPC listen address |
| `NODE_NAME` | `unknown` | Kubernetes node name |
| `JANUSD_MAX_GUARDS` | `1000` | Maximum concurrent guards |
| `JANUSD_AUDIT_ENABLED` | `false` | Enable audit logging |
| `LOG_LEVEL` | `info` | Log level |
| `RUST_LOG` | - | Rust log filter |

## Running

```bash
# Development
cargo run

# Production
./target/release/janusd

# With configuration
JANUSD_LISTEN_ADDR=0.0.0.0:50052 \
NODE_NAME=worker-1 \
LOG_LEVEL=debug \
./target/release/janusd
```

## Docker Build

```dockerfile
FROM rust:1.75 AS builder
WORKDIR /src
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /src/target/release/janusd /janusd
ENTRYPOINT ["/janusd"]
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
2. **Kernel audit integration** - Not fully implemented
3. **Policy caching** - Basic implementation
4. **Production testing** - Limited real-world validation

## Future Work

- [ ] Complete container runtime detection (containerd, CRI-O)
- [ ] Add kernel audit log integration
- [ ] Implement policy caching for performance
- [ ] Add comprehensive integration tests
- [ ] Performance optimization and benchmarking
- [ ] Production hardening

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../../LICENSE) for details.
