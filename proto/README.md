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
│   └── v1/
│       ├── README.md         # Argus API documentation
│       └── argus.proto       # Argus service definitions
└── janus/
    └── v1/
        ├── README.md         # Janus API documentation
        └── janus.proto       # Janus service definitions
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
# Generate Go code for operators
protoc --go_out=. --go-grpc_out=. \
  proto/argus/v1/argus.proto \
  proto/janus/v1/janus.proto
```

### C++ (Daemons)

```bash
# Generate C++ code for daemons
protoc --cpp_out=. --grpc_out=. \
  --plugin=protoc-gen-grpc=$(which grpc_cpp_plugin) \
  proto/argus/v1/argus.proto \
  proto/janus/v1/janus.proto
```

### Rust (Alternative Daemons)

Rust code is generated via `tonic-build` in `build.rs`:

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../proto/argus/v1/argus.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
```

## Versioning

Proto packages follow semantic versioning:

- `argus.v1` - Stable Argus API
- `janus.v1` - Stable Janus API

Breaking changes will be introduced in new major versions (v2, v3, etc.).

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
