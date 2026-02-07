# Panoptes Protocol Buffers

**gRPC service definitions for Argus and Janus daemons**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../LICENSE)

## Overview

This directory contains Protocol Buffer definitions for the gRPC services exposed by
the Argus and Janus daemons. These definitions are used to generate client and server
code for communication between Kubernetes operators and node-level daemons.

## Directory Structure

```
proto/
├── README.md                 # This file
├── argus/
│   ├── v2/                   # Current API version (recommended)
│   │   ├── README.md
│   │   └── argus.proto
│   └── v1/                   # Deprecated, for migration only
│       └── argus.proto
└── janus/
    ├── v2/                   # Current API version (recommended)
    │   ├── README.md
    │   └── janus.proto
    └── v1/                   # Deprecated, for migration only
        └── janus.proto
```

## Services

| Service | Port | Purpose |
|---------|------|---------|
| ArgusService | 50051 | File integrity monitoring |
| JanusService | 50052 | File access auditing |
| HealthService | Same | Health checks (both daemons) |

## Code Generation

### Go (Operators)

```bash
# Generate Go code for operators (v2 API)
protoc --go_out=. --go-grpc_out=. \
  proto/argus/v2/argus.proto \
  proto/janus/v2/janus.proto
```

### Rust (Daemons)

Rust code is generated via `tonic-build` in `build.rs`:

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../proto/argus/v2/argus.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
```

## Versioning

Proto packages follow semantic versioning:

- `argus.v2` / `janus.v2` - Current stable API (recommended)
- `argus.v1` / `janus.v1` - Deprecated, for migration only

## Common Patterns

### Health Checking

Both daemons implement the standard gRPC health checking protocol:

```protobuf
service HealthService {
  rpc Check(HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch(HealthCheckRequest) returns (stream HealthCheckResponse);
}
```

### Streaming Events

Both services support streaming for real-time event delivery:

```protobuf
// Argus
rpc StreamEvents(StreamEventsRequest) returns (stream FileEvent);

// Janus
rpc StreamEvents(StreamEventsRequest) returns (stream AccessEvent);
```

### Session Management

Both services use session-based resource management:

```protobuf
// Create returns a session ID
message CreateWatchResponse {
  int64 session_id = 1;
}

// Destroy uses the session ID
message DestroyWatchRequest {
  int64 session_id = 1;
}
```

## Proto3 Syntax

All definitions use proto3 syntax with the following conventions:

- Field numbers 1-15 for frequently used fields (1 byte encoding)
- Field numbers 16+ for less common fields
- Enums start at 0 (default/unknown value)
- Repeated fields for lists
- Maps for key-value pairs

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../LICENSE) for details.
