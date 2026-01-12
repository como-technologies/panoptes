# janusd - Janus File Access Auditing Daemon

**Node-level daemon for real-time file access control using Linux fanotify**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../LICENSE)

## Overview

janusd is the node-level daemon component of the Janus file access auditing system.
It runs as a DaemonSet on each Kubernetes node and provides:

- Direct fanotify kernel interface for file access interception
- Permission-based access control (allow/deny)
- gRPC API for receiving guard requests from the Janus operator
- Kernel audit log integration
- Container runtime integration (containerd, CRI-O)

### Original Implementation

The original janusd was created by [ClusterGarage](https://clustergarage.io/janus/) circa 2018.
This modernized version updates the build system and adds support for modern container runtimes.

## Implementations

janusd has two implementations:

| Implementation | Location | Status | Description |
|---------------|----------|--------|-------------|
| **C/C++** | `c/` | Primary | C core with C++ gRPC wrapper |
| **Rust** | `rust/` | Alternative | Pure Rust with nix/tonic |

The C implementation is the primary production version. The Rust implementation provides
an alternative with memory safety guarantees. See [rust/README.md](rust/README.md) for
the Rust version.

## How fanotify Differs from inotify

| Aspect | inotify (Argus) | fanotify (Janus) |
|--------|-----------------|------------------|
| Purpose | Notification | Permission control |
| Timing | After event | Before event completes |
| Response | None | FAN_ALLOW or FAN_DENY |
| Scope | Per-file | Per-mount |
| Permissions | Read-only | Can block access |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          janusd Architecture                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                         gRPC Server (C++)                          │ │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐ │ │
│  │  │  JanusService    │  │  HealthService   │  │  EventStream     │ │ │
│  │  │  • CreateGuard   │  │  • Check         │  │  • StreamEvents  │ │ │
│  │  │  • DestroyGuard  │  │  • Watch         │  │                  │ │ │
│  │  │  • UpdatePolicy  │  │                  │  │                  │ │ │
│  │  └────────┬─────────┘  └──────────────────┘  └────────┬─────────┘ │ │
│  │           │                                           │           │ │
│  └───────────┼───────────────────────────────────────────┼───────────┘ │
│              │                                           │             │
│              ▼                                           ▼             │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Core C Libraries                                │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │                     janusnotify                              │ │ │
│  │  │  • fanotify_init() - Initialize fanotify                     │ │ │
│  │  │  • fanotify_mark() - Add mount marks                         │ │ │
│  │  │  • read() - Read permission events                           │ │ │
│  │  │  • write() - Send FAN_ALLOW/FAN_DENY response                │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │                     janusaudit                               │ │ │
│  │  │  • audit_log_user_message() - Kernel audit integration       │ │ │
│  │  │  • Structured audit records for SIEM                         │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│              │                                                          │
│              ▼                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Linux Kernel                                  │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  fanotify subsystem                                          │ │ │
│  │  │  • FAN_OPEN_PERM - Permission request for open               │ │ │
│  │  │  • FAN_ACCESS_PERM - Permission request for read             │ │ │
│  │  │  • Response: FAN_ALLOW or FAN_DENY                           │ │ │
│  │  └──────────────────────────────────────────────────────────────┘ │ │
│  │  ┌──────────────────────────────────────────────────────────────┐ │ │
│  │  │  audit subsystem                                             │ │ │
│  │  │  • Structured audit log records                              │ │ │
│  │  │  • Integration with auditd                                   │ │ │
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
│   ├── janusnotify.h        # fanotify wrapper API
│   ├── janusaudit.h         # Kernel audit integration
│   └── container_runtime.h  # Container PID/rootfs lookup
├── lib/
│   ├── janusnotify.c        # Core fanotify implementation
│   ├── janusaudit.c         # Audit log implementation
│   └── container_runtime.c  # containerd/CRI-O integration
└── src/
    ├── main.cc              # C++ entry point
    ├── janusd_impl.cc       # gRPC service implementation
    ├── janusd_impl.h
    ├── health_impl.cc       # Health check service
    └── health_impl.h
```

### What's Original vs Updated

**Original ClusterGarage code (preserved):**
- `lib/janusnotify.c` - Core fanotify wrapper (direct kernel syscalls)
- `lib/janusaudit.c` - Kernel audit log integration
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
- libaudit (for kernel audit integration)

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

# The binary is at build/janusd
```

### gRPC API

janusd exposes a gRPC API on port 50052 (configurable):

```protobuf
service JanusService {
  // Create a new access guard
  rpc CreateGuard(CreateGuardRequest) returns (CreateGuardResponse);

  // Destroy an existing guard
  rpc DestroyGuard(DestroyGuardRequest) returns (DestroyGuardResponse);

  // Get guard status
  rpc GetGuardStatus(GetGuardStatusRequest) returns (GetGuardStatusResponse);

  // List all active guards
  rpc ListGuards(ListGuardsRequest) returns (ListGuardsResponse);

  // Update guard policy (allow/deny patterns)
  rpc UpdatePolicy(UpdatePolicyRequest) returns (UpdatePolicyResponse);

  // Stream access events
  rpc StreamEvents(StreamEventsRequest) returns (stream AccessEvent);
}

service HealthService {
  rpc Check(HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch(HealthCheckRequest) returns (stream HealthCheckResponse);
}
```

See [proto/janus/v1/README.md](../../proto/janus/v1/README.md) for full API documentation.

### Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `JANUSD_LISTEN_ADDR` | `0.0.0.0:50052` | gRPC listen address |
| `JANUSD_MAX_GUARDS` | `1000` | Maximum concurrent guards |
| `JANUSD_AUDIT_ENABLED` | `false` | Enable kernel audit logging |
| `JANUSD_LOG_LEVEL` | `info` | Log level (trace, debug, info, warn, error) |
| `JANUSD_LOG_FORMAT` | `json` | Log format (json, text) |
| `NODE_NAME` | `unknown` | Kubernetes node name |

### Required Capabilities

janusd requires elevated capabilities for fanotify and audit:

```yaml
securityContext:
  capabilities:
    add:
      - SYS_ADMIN      # fanotify_init with FAN_CLASS_CONTENT
      - SYS_PTRACE     # Access /proc/{pid}/root
      - DAC_READ_SEARCH # Traverse container filesystems
      - AUDIT_WRITE    # Write to kernel audit log
```

### Container Runtime Support

janusd automatically detects and supports:

| Runtime | Socket Path | PID Location |
|---------|-------------|--------------|
| containerd | `/run/containerd/containerd.sock` | `/run/containerd/io.containerd.runtime.v2.task/k8s.io/{id}/init.pid` |
| CRI-O | `/var/run/crio/crio.sock` | `/var/run/crio/crio/{id}/pidfile` |

Docker and rkt support has been removed (deprecated in Kubernetes).

### Event Types

| Event | fanotify Flag | Description |
|-------|---------------|-------------|
| `open` | FAN_OPEN_PERM | Permission request for file open |
| `access` | FAN_ACCESS_PERM | Permission request for file read |

### Response Types

| Response | fanotify Response | Description |
|----------|------------------|-------------|
| `allow` | FAN_ALLOW | Permit the access |
| `deny` | FAN_DENY | Block the access (returns EPERM) |
| `audit` | FAN_ALLOW + audit log | Allow but log to audit |

### Policy Evaluation

janusd evaluates access requests against policies:

1. Check deny patterns first (if match, return DENY)
2. Check allow patterns (if match, return ALLOW)
3. Default: If deny patterns exist, DENY; otherwise ALLOW

```c
// Simplified policy evaluation
response_t evaluate_access(const char* path, policy_t* policy) {
    // Check deny patterns first
    for (int i = 0; i < policy->deny_count; i++) {
        if (glob_match(policy->deny[i], path)) {
            return RESPONSE_DENY;
        }
    }

    // Check allow patterns
    for (int i = 0; i < policy->allow_count; i++) {
        if (glob_match(policy->allow[i], path)) {
            return RESPONSE_ALLOW;
        }
    }

    // Default: deny if deny patterns exist, otherwise allow
    return policy->deny_count > 0 ? RESPONSE_DENY : RESPONSE_ALLOW;
}
```

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
cd daemons/janusd

# Use Rust implementation (default)
ln -sf Dockerfile.rust Dockerfile

# Use C implementation
ln -sf Dockerfile.c Dockerfile
```

**Rebuild after switching:**

```bash
# From repository root (build context needs proto/ and daemons/common/)
docker build -t janusd:2.0.0 -f daemons/janusd/Dockerfile .
```

### Multi-stage Build

The Dockerfile uses a multi-stage build:

1. **Builder stage**: Ubuntu 24.04 with build tools (C) or Rust 1.75 (Rust)
2. **Runtime stage**: Minimal image for minimal attack surface

```bash
# Build image (from repo root)
docker build -t janusd:2.0.0 -f daemons/janusd/Dockerfile .

# Run (requires privileged for fanotify)
docker run --privileged -p 50052:50052 janusd:2.0.0
```

### Image Details

| Property | Value |
|----------|-------|
| Base | `gcr.io/distroless/cc-debian12:nonroot` |
| User | nonroot (65532) |
| Size | ~15MB |
| Entrypoint | `/janusd` |

## Kernel Audit Integration

When `JANUSD_AUDIT_ENABLED=true`, janusd writes structured records to the kernel
audit subsystem:

```
type=FANOTIFY msg=audit(1704883800.123:456):
  operation="open"
  path="/etc/shadow"
  response="deny"
  pid=12345
  guard="protect-system"
  namespace="production"
  pod="my-app-abc123"
```

These records can be:
- Collected by auditd
- Forwarded to SIEM systems
- Used for compliance reporting

## Monitoring

### Health Checks

```bash
# gRPC health check
grpcurl -plaintext localhost:50052 grpc.health.v1.Health/Check

# Kubernetes probes
livenessProbe:
  grpc:
    port: 50052
readinessProbe:
  grpc:
    port: 50052
```

### Metrics

janusd exposes Prometheus metrics on `/metrics`:

| Metric | Type | Description |
|--------|------|-------------|
| `janusd_guards_total` | Gauge | Current active guards |
| `janusd_access_events_total` | Counter | Total events by response |
| `janusd_denied_events_total` | Counter | Total denied accesses |
| `janusd_event_latency_seconds` | Histogram | Event processing latency |
| `janusd_grpc_requests_total` | Counter | gRPC requests by method |

### Logging

Structured JSON logs:

```json
{
  "timestamp": "2026-01-10T10:30:00.123Z",
  "level": "info",
  "event": "access_denied",
  "path": "/etc/shadow",
  "response": "deny",
  "pid": 12345,
  "guard": "protect-system",
  "pod": "my-app-abc123"
}
```

## Troubleshooting

### fanotify initialization failed

```
fanotify_init: Operation not permitted
```

Ensure the daemon has `CAP_SYS_ADMIN` capability and is running with sufficient
privileges. fanotify with `FAN_CLASS_CONTENT` (for permission events) requires
elevated privileges.

### Permission denied accessing container filesystem

Ensure the daemon has required capabilities and is running with `hostPID: true`.

### High latency on file access

fanotify permission events are synchronous - the accessing process blocks until
janusd responds. Optimize by:

1. Limiting guarded paths
2. Using efficient glob patterns
3. Monitoring `janusd_event_latency_seconds`

### Audit buffer overflow

When audit is enabled with high event rates:

```bash
# Check for lost events
dmesg | grep audit

# Increase buffer
auditctl -b 8192
```

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.

## Acknowledgments

Based on the original [janusd](https://github.com/clustergarage/janusd) by ClusterGarage.
