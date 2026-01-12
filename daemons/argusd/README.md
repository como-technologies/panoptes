# argusd - Argus File Integrity Monitoring Daemon

**Node-level daemon for real-time file system monitoring using Linux inotify**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../LICENSE)

## Overview

argusd is the node-level daemon component of the Argus file integrity monitoring system.
It runs as a DaemonSet on each Kubernetes node and provides:

- Direct inotify kernel interface for file system events
- gRPC API for receiving watch requests from the Argus operator
- Container runtime integration (containerd, CRI-O) for pod filesystem access
- Structured event logging with customizable formats

### Original Implementation

The original argusd was created by [ClusterGarage](https://clustergarage.io/argus/) circa 2018.
This modernized version updates the build system and adds support for modern container runtimes.

## Implementations

argusd has two implementations:

| Implementation | Location | Status | Description |
|---------------|----------|--------|-------------|
| **C/C++** | `c/` | Primary | C core with C++ gRPC wrapper |
| **Rust** | `rust/` | Alternative | Pure Rust with nix/tonic |

The C implementation is the primary production version. The Rust implementation provides
an alternative with memory safety guarantees. See [rust/README.md](rust/README.md) for
the Rust version.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          argusd Architecture                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                         gRPC Server (C++)                          │ │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐ │ │
│  │  │  ArgusService    │  │  HealthService   │  │  EventStream     │ │ │
│  │  │  • CreateWatch   │  │  • Check         │  │  • StreamEvents  │ │ │
│  │  │  • DestroyWatch  │  │  • Watch         │  │                  │ │ │
│  │  │  • ListWatches   │  │                  │  │                  │ │ │
│  │  └────────┬─────────┘  └──────────────────┘  └────────┬─────────┘ │ │
│  │           │                                           │           │ │
│  └───────────┼───────────────────────────────────────────┼───────────┘ │
│              │                                           │             │
│              ▼                                           ▼             │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Core C Libraries                                │ │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐ │ │
│  │  │   argusnotify    │  │    argustree     │  │    arguscache    │ │ │
│  │  │  • inotify_init  │  │  • Recursive     │  │  • Event         │ │ │
│  │  │  • add_watch     │  │    directory     │  │    deduplication │ │ │
│  │  │  • read_events   │  │    tracking      │  │  • Batching      │ │ │
│  │  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘ │ │
│  │           │                     │                     │           │ │
│  └───────────┼─────────────────────┼─────────────────────┼───────────┘ │
│              │                     │                     │             │
│              └─────────────────────┼─────────────────────┘             │
│                                    │                                   │
│                                    ▼                                   │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                  Container Runtime Integration                     │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  container_runtime.c                                         │ │ │
│  │  │  • detect_runtime() - containerd vs CRI-O                    │ │ │
│  │  │  • get_container_pid() - Find container init PID             │ │ │
│  │  │  • get_container_rootfs() - /proc/{pid}/root path            │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                   │
│                                    ▼                                   │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Linux Kernel                                  │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  inotify subsystem                                           │ │ │
│  │  │  • IN_ACCESS, IN_MODIFY, IN_CREATE, IN_DELETE                │ │ │
│  │  │  • IN_ATTRIB, IN_CLOSE_WRITE, IN_CLOSE_NOWRITE              │ │ │
│  │  │  • IN_MOVED_FROM, IN_MOVED_TO, IN_OPEN                       │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

## C Implementation

### Directory Structure

```
c/
├── CMakeLists.txt           # Modern CMake 3.20+ build
├── include/
│   ├── argusnotify.h        # inotify wrapper API
│   ├── argustree.h          # Recursive directory tracking
│   ├── arguscache.h         # Event caching and deduplication
│   └── container_runtime.h  # Container PID/rootfs lookup
├── lib/
│   ├── argusnotify.c        # Core inotify implementation
│   ├── argustree.c          # Directory tree management
│   ├── arguscache.c         # Event cache implementation
│   └── container_runtime.c  # containerd/CRI-O integration
└── src/
    ├── main.cc              # C++ entry point
    ├── argusd_impl.cc       # gRPC service implementation
    ├── argusd_impl.h
    ├── health_impl.cc       # Health check service
    └── health_impl.h
```

### What's Original vs Updated

**Original ClusterGarage code (preserved):**
- `lib/argusnotify.c` - Core inotify wrapper (direct kernel syscalls)
- `lib/argustree.c` - Recursive directory watch management
- `lib/arguscache.c` - Event caching logic
- Core C APIs in `include/*.h`

**Modernized/Updated:**
- `CMakeLists.txt` - New CMake 3.20+ build (was Makefile)
- `lib/container_runtime.c` - New: containerd/CRI-O support (was Docker/rkt)
- `src/*.cc` - Updated gRPC 1.60+ wrapper
- Build uses C17 standard and C++20 for gRPC wrapper
- spdlog for structured JSON logging (was custom logging)

### Building

#### Prerequisites

- CMake 3.20+
- GCC 11+ or Clang 14+
- gRPC 1.60+ and Protobuf
- spdlog 1.12+

#### Build Commands

```bash
cd c/

# Configure
cmake -B build \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_C_STANDARD=17 \
  -DCMAKE_CXX_STANDARD=20

# Build
cmake --build build -j$(nproc)

# The binary is at build/argusd
```

#### Build Options

| Option | Default | Description |
|--------|---------|-------------|
| `CMAKE_BUILD_TYPE` | Debug | Release, Debug, RelWithDebInfo |
| `ENABLE_ASAN` | OFF | Enable AddressSanitizer |
| `ENABLE_TSAN` | OFF | Enable ThreadSanitizer |

### gRPC API

argusd exposes a gRPC API on port 50051 (configurable):

```protobuf
service ArgusService {
  // Create a new file watch
  rpc CreateWatch(CreateWatchRequest) returns (CreateWatchResponse);

  // Destroy an existing watch
  rpc DestroyWatch(DestroyWatchRequest) returns (DestroyWatchResponse);

  // List all active watches
  rpc ListWatches(ListWatchesRequest) returns (ListWatchesResponse);

  // Stream file events
  rpc StreamEvents(StreamEventsRequest) returns (stream FileEvent);
}

service HealthService {
  rpc Check(HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch(HealthCheckRequest) returns (stream HealthCheckResponse);
}
```

See [proto/argus/v1/README.md](../../proto/argus/v1/README.md) for full API documentation.

### Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `ARGUSD_LISTEN_ADDR` | `0.0.0.0:50051` | gRPC listen address |
| `ARGUSD_MAX_WATCHES` | `10000` | Maximum inotify watches |
| `ARGUSD_CACHE_SIZE` | `1000` | Event cache size |
| `ARGUSD_LOG_LEVEL` | `info` | Log level (trace, debug, info, warn, error) |
| `ARGUSD_LOG_FORMAT` | `json` | Log format (json, text) |
| `NODE_NAME` | `unknown` | Kubernetes node name |

### Required Capabilities

argusd requires elevated capabilities to access container filesystems:

```yaml
securityContext:
  capabilities:
    add:
      - SYS_ADMIN      # inotify on container mounts
      - SYS_PTRACE     # Access /proc/{pid}/root
      - DAC_READ_SEARCH # Traverse container filesystems
```

### Container Runtime Support

argusd automatically detects and supports:

| Runtime | Socket Path | PID Location |
|---------|-------------|--------------|
| containerd | `/run/containerd/containerd.sock` | `/run/containerd/io.containerd.runtime.v2.task/k8s.io/{id}/init.pid` |
| CRI-O | `/var/run/crio/crio.sock` | `/var/run/crio/crio/{id}/pidfile` |

Docker and rkt support has been removed (deprecated in Kubernetes).

### Event Types

| Event | inotify Flag | Description |
|-------|--------------|-------------|
| `access` | IN_ACCESS | File was accessed (read) |
| `attrib` | IN_ATTRIB | Metadata changed |
| `closewrite` | IN_CLOSE_WRITE | File closed after writing |
| `closenowrite` | IN_CLOSE_NOWRITE | File closed without writing |
| `create` | IN_CREATE | File/directory created |
| `delete` | IN_DELETE | File/directory deleted |
| `deleteself` | IN_DELETE_SELF | Watched item deleted |
| `modify` | IN_MODIFY | File was modified |
| `moveself` | IN_MOVE_SELF | Watched item moved |
| `movedfrom` | IN_MOVED_FROM | File moved out |
| `movedto` | IN_MOVED_TO | File moved in |
| `open` | IN_OPEN | File was opened |

## Docker Image

### Switching Between C and Rust Implementations

The daemon has two Docker build configurations:

| Implementation | Dockerfile | Description |
|----------------|------------|-------------|
| **C/C++** | `Dockerfile.c` | Primary production version, fully static binary |
| **Rust** | `Dockerfile.rust` | Alternative implementation with memory safety |

The `Dockerfile` is a symlink that points to the active implementation (defaults to Rust).

**To switch implementations:**

```bash
cd daemons/argusd

# Use Rust implementation (default)
ln -sf Dockerfile.rust Dockerfile

# Use C implementation
ln -sf Dockerfile.c Dockerfile
```

**Rebuild after switching:**

```bash
# From repository root (build context needs proto/ and daemons/common/)
docker build -t argusd:2.0.0 -f daemons/argusd/Dockerfile .
```

### Multi-stage Build

The Dockerfile uses a multi-stage build:

1. **Builder stage**: Ubuntu 24.04 with build tools (C) or Rust 1.75 (Rust)
2. **Runtime stage**: Minimal image for minimal attack surface

```bash
# Build image (from repo root)
docker build -t argusd:2.0.0 -f daemons/argusd/Dockerfile .

# Run (requires privileged for inotify)
docker run --privileged -p 50051:50051 argusd:2.0.0
```

### Image Details

| Property | Value |
|----------|-------|
| Base | `gcr.io/distroless/cc-debian12:nonroot` |
| User | nonroot (65532) |
| Size | ~15MB |
| Entrypoint | `/argusd` |

## Monitoring

### Health Checks

```bash
# gRPC health check
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check

# Kubernetes probes
livenessProbe:
  grpc:
    port: 50051
readinessProbe:
  grpc:
    port: 50051
```

### Metrics

argusd exposes Prometheus metrics on `/metrics`:

| Metric | Type | Description |
|--------|------|-------------|
| `argusd_watches_total` | Gauge | Current active watches |
| `argusd_events_total` | Counter | Total events by type |
| `argusd_event_latency_seconds` | Histogram | Event processing latency |
| `argusd_grpc_requests_total` | Counter | gRPC requests by method |

### Logging

Structured JSON logs:

```json
{
  "timestamp": "2026-01-10T10:30:00.123Z",
  "level": "info",
  "event": "file_modified",
  "path": "/etc/passwd",
  "watch_id": 42,
  "pod": "my-app-abc123",
  "container": "main"
}
```

## Troubleshooting

### Maximum watches exceeded

```
inotify_add_watch: No space left on device
```

Increase the system limit:
```bash
echo 524288 | sudo tee /proc/sys/fs/inotify/max_user_watches
# Make persistent
echo "fs.inotify.max_user_watches=524288" | sudo tee -a /etc/sysctl.conf
```

### Permission denied accessing container filesystem

Ensure the daemon has required capabilities and is running with `hostPID: true`.

### Events not being received

1. Check if the container runtime socket exists
2. Verify the container PID can be resolved
3. Check kernel inotify subsystem: `cat /proc/sys/fs/inotify/max_user_watches`

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.

## Acknowledgments

Based on the original [argusd](https://github.com/clustergarage/argusd) by ClusterGarage.
