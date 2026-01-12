# Janus gRPC API v1

**Protocol Buffer definitions for the Janus file access auditing daemon**

## Overview

This package defines the gRPC API for communication between the Janus operator
and janusd daemons. The API enables creating file access guards, streaming events,
updating policies, and managing guard lifecycle.

## Services

### JanusService

The main service for file access auditing and enforcement.

```protobuf
service JanusService {
  // Create a new access guard on container filesystem
  rpc CreateGuard(CreateGuardRequest) returns (CreateGuardResponse);

  // Destroy an existing guard
  rpc DestroyGuard(DestroyGuardRequest) returns (DestroyGuardResponse);

  // Get status of a specific guard
  rpc GetGuardStatus(GetGuardStatusRequest) returns (GetGuardStatusResponse);

  // List all active guards
  rpc ListGuards(ListGuardsRequest) returns (ListGuardsResponse);

  // Update guard policy (allow/deny patterns)
  rpc UpdatePolicy(UpdatePolicyRequest) returns (UpdatePolicyResponse);

  // Stream access events in real-time
  rpc StreamEvents(StreamEventsRequest) returns (stream AccessEvent);
}
```

### HealthService

Standard gRPC health checking.

```protobuf
service HealthService {
  rpc Check(HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch(HealthCheckRequest) returns (stream HealthCheckResponse);
}
```

## Messages

### CreateGuardRequest

Request to create a new file access guard.

```protobuf
message CreateGuardRequest {
  // Guard resource name (e.g., "default/my-guard")
  string guard_name = 1;

  // Kubernetes namespace
  string namespace = 2;

  // Pod name being guarded
  string pod_name = 3;

  // Container ID (e.g., "containerd://abc123")
  string container_id = 4;

  // Guard mode
  GuardMode mode = 5;

  // Guard options
  GuardOptions options = 6;
}

enum GuardMode {
  // Unknown/default mode
  GUARD_MODE_UNSPECIFIED = 0;

  // Audit only - log but don't block
  GUARD_MODE_AUDIT = 1;

  // Enforce - block denied access
  GUARD_MODE_ENFORCE = 2;
}

message GuardOptions {
  // Allowed path patterns (glob)
  repeated string allow_patterns = 1;

  // Denied path patterns (glob)
  repeated string deny_patterns = 2;

  // Events to guard (access, open)
  repeated string events = 3;

  // Only guard directory access
  bool only_dir = 4;

  // Allow file owner to access
  bool auto_allow_owner = 5;

  // Log to kernel audit subsystem
  bool audit = 6;

  // Custom metadata tags
  map<string, string> tags = 7;
}
```

### CreateGuardResponse

```protobuf
message CreateGuardResponse {
  // Unique session ID for this guard
  int64 session_id = 1;

  // Error message (empty on success)
  string error = 2;
}
```

### AccessEvent

A file access event detected by fanotify.

```protobuf
message AccessEvent {
  // Session this event belongs to
  int64 session_id = 1;

  // Event type (access, open)
  string event_type = 2;

  // Full path to the file
  string path = 3;

  // Filename only
  string filename = 4;

  // Whether path is a directory
  bool is_dir = 5;

  // Process ID that triggered the access
  int32 pid = 6;

  // User ID of the process
  uint32 uid = 7;

  // Response sent to kernel
  AccessResponse response = 8;

  // Timestamp (Unix nanoseconds)
  int64 timestamp = 9;

  // Metadata from guard configuration
  map<string, string> tags = 10;

  // Guard resource name
  string guard_name = 11;

  // Pod name
  string pod_name = 12;

  // Container ID
  string container_id = 13;
}

enum AccessResponse {
  // Unknown response
  ACCESS_RESPONSE_UNSPECIFIED = 0;

  // Access was allowed
  ACCESS_RESPONSE_ALLOW = 1;

  // Access was denied
  ACCESS_RESPONSE_DENY = 2;

  // Access was allowed and audited
  ACCESS_RESPONSE_AUDIT = 3;
}
```

### UpdatePolicyRequest

Update allow/deny patterns for an active guard.

```protobuf
message UpdatePolicyRequest {
  // Session ID of the guard to update
  int64 session_id = 1;

  // Guard resource name
  string guard_name = 2;

  // New allowed patterns (replaces existing)
  repeated string allow_patterns = 3;

  // New denied patterns (replaces existing)
  repeated string deny_patterns = 4;
}
```

### GetGuardStatusResponse

```protobuf
message GetGuardStatusResponse {
  // Guard is active
  bool active = 1;

  // Guard mode
  GuardMode mode = 2;

  // Total allowed events
  int64 allowed_events = 3;

  // Total denied events
  int64 denied_events = 4;

  // Total audited events
  int64 audited_events = 5;

  // Current allow patterns
  repeated string allow_patterns = 6;

  // Current deny patterns
  repeated string deny_patterns = 7;

  // Error message (empty on success)
  string error = 8;
}
```

### StreamEventsRequest

```protobuf
message StreamEventsRequest {
  // Filter by session ID (0 for all)
  int64 session_id = 1;

  // Filter by guard name (empty for all)
  string guard_name = 2;

  // Filter by pod name (empty for all)
  string pod_name = 3;

  // Filter to denied events only
  bool denied_only = 4;
}
```

## Event Types

| Event Type | fanotify Flag | Description |
|------------|---------------|-------------|
| `access` | FAN_ACCESS_PERM | Permission request for file read |
| `open` | FAN_OPEN_PERM | Permission request for file open |

## Policy Evaluation

When a file access request is received, janusd evaluates policies in order:

1. **Check deny patterns** - If path matches any deny pattern, return DENY
2. **Check allow patterns** - If path matches any allow pattern, return ALLOW
3. **Default behavior**:
   - If deny patterns exist, return DENY
   - Otherwise, return ALLOW

### Glob Pattern Syntax

Patterns support standard glob syntax:

| Pattern | Description |
|---------|-------------|
| `*` | Match any sequence of characters (not including `/`) |
| `**` | Match any sequence including `/` |
| `?` | Match any single character |
| `[abc]` | Match any character in brackets |
| `[!abc]` | Match any character not in brackets |

Examples:
- `/etc/shadow` - Exact match
- `/etc/*` - All files in /etc/
- `/etc/**` - All files recursively under /etc/
- `*.conf` - All .conf files
- `/home/*/.ssh/**` - All files in any user's .ssh directory

## Usage Examples

### Go Client

```go
import (
    pb "github.com/como-technologies/panoptes/proto/janus/v1"
    "google.golang.org/grpc"
)

conn, _ := grpc.Dial("localhost:50052", grpc.WithInsecure())
client := pb.NewJanusServiceClient(conn)

// Create guard
resp, _ := client.CreateGuard(ctx, &pb.CreateGuardRequest{
    GuardName:   "default/my-guard",
    Namespace:   "default",
    PodName:     "my-app-abc123",
    ContainerId: "containerd://xyz",
    Mode:        pb.GuardMode_GUARD_MODE_ENFORCE,
    Options: &pb.GuardOptions{
        AllowPatterns: []string{"/app/**", "/tmp/**"},
        DenyPatterns:  []string{"/etc/shadow", "/root/**"},
        Events:        []string{"open", "access"},
        Audit:         true,
    },
})

sessionId := resp.SessionId

// Stream denied events
stream, _ := client.StreamEvents(ctx, &pb.StreamEventsRequest{
    SessionId:  sessionId,
    DeniedOnly: true,
})

for {
    event, err := stream.Recv()
    if err == io.EOF {
        break
    }
    log.Printf("Denied: %s on %s (pid=%d)",
        event.EventType, event.Path, event.Pid)
}
```

### Update Policy

```go
// Update policy without recreating guard
_, err := client.UpdatePolicy(ctx, &pb.UpdatePolicyRequest{
    SessionId:     sessionId,
    GuardName:     "default/my-guard",
    AllowPatterns: []string{"/app/**", "/tmp/**", "/var/log/**"},
    DenyPatterns:  []string{"/etc/shadow"},
})
```

### Rust Client

```rust
use janus::v1::janus_service_client::JanusServiceClient;

let mut client = JanusServiceClient::connect("http://localhost:50052").await?;

let response = client.create_guard(CreateGuardRequest {
    guard_name: "default/my-guard".into(),
    namespace: "default".into(),
    pod_name: "my-app-abc123".into(),
    container_id: "containerd://xyz".into(),
    mode: GuardMode::Enforce as i32,
    options: Some(GuardOptions {
        allow_patterns: vec!["/app/**".into()],
        deny_patterns: vec!["/etc/shadow".into()],
        events: vec!["open".into()],
        audit: true,
        ..Default::default()
    }),
}).await?;

let session_id = response.into_inner().session_id;
```

## Error Handling

All responses include an `error` field that is empty on success:

```protobuf
message DestroyGuardResponse {
  bool success = 1;
  string error = 2;  // Empty on success
}
```

Common error messages:
- `"session not found"` - Invalid session ID
- `"fanotify init failed"` - Insufficient capabilities
- `"permission denied"` - Insufficient capabilities for FAN_CLASS_CONTENT
- `"max guards exceeded"` - Guard limit reached

## Kernel Audit Integration

When `audit: true` is set in guard options, janusd writes structured records
to the kernel audit subsystem for each access event:

```
type=FANOTIFY msg=audit(1704883800.123:456):
  operation="open"
  path="/etc/shadow"
  response="deny"
  pid=12345
  uid=1000
  guard="default/my-guard"
  namespace="default"
  pod="my-app-abc123"
```

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
