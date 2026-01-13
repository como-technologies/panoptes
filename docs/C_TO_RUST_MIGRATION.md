# C to Rust Daemon Migration

This document describes the migration from dual C/Rust daemon implementations to a Rust-only codebase.

## Background

The Panoptes daemons (argusd and janusd) were originally implemented in C/C++, based on the ClusterGarage Argus and Janus projects. A parallel Rust implementation was developed as an alternative, offering modern async capabilities, memory safety, and eBPF integration.

As of this migration, the Rust implementation has become the sole implementation, and the C/C++ code has been removed from the repository.

## What Changed

### Removed Directories

| Path | Description |
|------|-------------|
| `daemons/argusd/c/` | C/C++ inotify FIM implementation |
| `daemons/janusd/c/` | C/C++ fanotify audit implementation |
| `daemons/common/lib/` | Shared C library (container runtime detection) |
| `daemons/common/cmake/` | CMake dependency configuration (gflags, glog, fmt) |
| `benchmarks/` | C vs Rust performance comparison infrastructure |

### Removed Files

| Path | Description |
|------|-------------|
| `daemons/argusd/Dockerfile.c` | Multi-stage C build Dockerfile |
| `daemons/janusd/Dockerfile.c` | Multi-stage C build Dockerfile |

### Simplified Structure

The Rust code was moved from subdirectories to the daemon root:

| Before | After |
|--------|-------|
| `daemons/argusd/rust/Cargo.toml` | `daemons/argusd/Cargo.toml` |
| `daemons/argusd/rust/src/` | `daemons/argusd/src/` |
| `daemons/argusd/rust/ebpf/` | `daemons/argusd/ebpf/` |
| `daemons/argusd/Dockerfile.rust` | `daemons/argusd/Dockerfile` |
| `daemons/janusd/rust/Cargo.toml` | `daemons/janusd/Cargo.toml` |
| `daemons/janusd/rust/src/` | `daemons/janusd/src/` |
| `daemons/janusd/rust/ebpf/` | `daemons/janusd/ebpf/` |
| `daemons/janusd/Dockerfile.rust` | `daemons/janusd/Dockerfile` |
| `daemons/common/rust/Cargo.toml` | `daemons/common/Cargo.toml` |
| `daemons/common/rust/src/` | `daemons/common/src/` |

## New Directory Structure

```
daemons/
├── argusd/
│   ├── Cargo.toml           # Rust project manifest
│   ├── Cargo.lock
│   ├── Dockerfile           # Multi-stage Rust build
│   ├── README.md
│   ├── build.rs             # Proto code generation
│   ├── benches/             # Criterion benchmarks
│   ├── ebpf/                # eBPF kernel programs
│   ├── src/                 # Rust source code
│   └── tests/               # Integration tests
├── janusd/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── Dockerfile
│   ├── README.md
│   ├── build.rs
│   ├── benches/
│   ├── ebpf/
│   ├── src/
│   └── tests/
└── common/
    ├── Cargo.toml           # panoptes-common crate
    ├── Cargo.lock
    ├── EBPF.md              # eBPF documentation
    └── src/                 # Shared utilities
```

## Updated Build Commands

### Building Daemons

```bash
# Build argusd
cd daemons/argusd
cargo build --release

# Build janusd
cd daemons/janusd
cargo build --release

# Build with eBPF support
cargo build --release --features ebpf
```

### Building Docker Images

```bash
# From repository root
docker build -f daemons/argusd/Dockerfile -t argusd:latest .
docker build -f daemons/janusd/Dockerfile -t janusd:latest .

# With eBPF feature
docker build --build-arg FEATURES=ebpf -f daemons/argusd/Dockerfile -t argusd:ebpf .
```

### Running Tests

```bash
cd daemons/argusd
cargo test

cd daemons/janusd
cargo test

# With clippy and formatting
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Rust Implementation Features

The Rust implementation provides all the functionality of the C version plus:

| Feature | Description |
|---------|-------------|
| Async I/O | Tokio-based async runtime for efficient event processing |
| Memory Safety | No buffer overflows, use-after-free, or data races |
| eBPF Integration | Optional LSM-based monitoring with kernel-side filtering |
| Smaller Binaries | ~5-8 MB static binaries (musl) vs ~15 MB for C |
| Cross-compilation | Easy static builds with `x86_64-unknown-linux-musl` |
| Modern Error Handling | `Result`/`Option` types with `thiserror` |

## Migration Verification

After updating, verify the migration:

1. **Build locally:**
   ```bash
   cd daemons/argusd && cargo build --release
   cd daemons/janusd && cargo build --release
   ```

2. **Run tests:**
   ```bash
   cd daemons/argusd && cargo test
   cd daemons/janusd && cargo test
   ```

3. **Build Docker images:**
   ```bash
   docker build -f daemons/argusd/Dockerfile -t argusd:test .
   docker build -f daemons/janusd/Dockerfile -t janusd:test .
   ```

4. **Deploy to local cluster:**
   ```bash
   ./hack/local-deploy.sh all
   ```

## CI/CD Changes

The GitHub Actions workflows have been updated:

- Removed C build jobs (`build-argusd-c`, `build-janusd-c`)
- Renamed Rust jobs to remove `-rust` suffix
- Updated Docker build contexts and Dockerfile paths
- Simplified job dependencies

## Historical Reference

The C implementation was preserved in source control before this migration. To access the historical C code, check out any commit prior to this migration.

The C implementation used:
- C99/C17 for core libraries (inotify/fanotify wrappers)
- C++17/C++20 for gRPC server implementation
- CMake 3.20+ build system
- gRPC 1.60+ with static linking
- gflags, glog, fmt for utilities
