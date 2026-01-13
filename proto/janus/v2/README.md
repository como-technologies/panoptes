# Janus gRPC API v2

**Protocol Buffer definitions for the Janus file access auditing daemon (Rust)**

## Overview

This package defines the gRPC API for the Rust implementation of the Janus daemon.
The API enables creating file access guards, streaming events, updating policies,
and managing guard lifecycle.

**V2 is the current API** used by the Rust daemon implementation. V1 is the legacy
API from the original C daemon and is retained for reference only.

### Key Differences from V1

| Feature | V1 | V2 |
|---------|----|----|
| Implementation | C daemon | Rust daemon |
| Package | `janus.v1` | `janus.v2` |
| Service name | `JanusService` | `JanusdService` |
| Event types | String-based | Typed enum (`FanotifyEvent`) |
| Timestamps | `int64` nanoseconds | `google.protobuf.Timestamp` |
| Process info | Basic (pid, uid) | Extended: `ppid`, `cmdline`, `cwd` |
| Guard control | Create/Destroy only | `UpdateGuard` for pause/resume |
| Policy updates | Recreate guard | `UpdatePolicy` for live updates |

## Services

### JanusdService

The main service for file access auditing and enforcement.

```protobuf
service JanusdService {
  // Create a new file access guard
  rpc CreateGuard(CreateGuardRequest) returns (CreateGuardResponse);

  // Remove an existing guard
  rpc DestroyGuard(DestroyGuardRequest) returns (google.protobuf.Empty);

  // Get current state of all guards on this node
  rpc GetGuardState(GetGuardStateRequest) returns (stream GuardState);

  // Stream access events in real-time
  rpc StreamAccessEvents(StreamAccessEventsRequest) returns (stream AccessEvent);

  // Get metrics for monitoring
  rpc GetMetrics(GetMetricsRequest) returns (MetricsResponse);

  // Pause or resume an existing guard (NEW in v2)
  rpc UpdateGuard(UpdateGuardRequest) returns (UpdateGuardResponse);

  // Update allow/deny patterns without recreating guard (NEW in v2)
  rpc UpdatePolicy(UpdatePolicyRequest) returns (UpdatePolicyResponse);
}
```

## Messages

### CreateGuardRequest

Request to create a new file access guard.

```protobuf
message CreateGuardRequest {
  string guard_name = 1;         // JanusGuard resource name
  string namespace = 2;          // Kubernetes namespace
  string node_name = 3;          // Kubernetes node name
  string pod_name = 4;           // Pod being guarded
  repeated string container_ids = 5;  // Container IDs to guard
  repeated int32 pids = 6;       // Process IDs (from containers)
  repeated GuardSubject subjects = 7; // Access control rules
  string log_format = 8;         // Custom log format template
  bool paused = 9;               // Start in paused state
  bool enforcing = 10;           // Enforce denials (vs audit-only)
}

message GuardSubject {
  repeated string allow = 1;     // Paths to explicitly allow
  repeated string deny = 2;      // Paths to explicitly deny
  repeated FanotifyEvent events = 3;  // Event types to monitor
  bool only_dir = 4;             // Only monitor directories
  bool auto_allow_owner = 5;     // Allow access for process owner
  bool audit = 6;                // Write to kernel audit log
  AccessResponse default_response = 7;  // Action when no rule matches
  map<string, string> tags = 8;  // Custom metadata
}
```

### CreateGuardResponse

```protobuf
message CreateGuardResponse {
  string guard_id = 1;           // Unique guard identifier
  string node_name = 2;          // Node where guard was created
  string pod_name = 3;           // Pod being guarded
  int32 guarded_paths = 4;       // Number of paths being guarded
  repeated int32 process_eventfds = 5;  // Event FDs for process tracking
  bool paused = 6;               // Whether guard is paused
  bool enforcing = 7;            // Whether guard is enforcing
  int32 marks_registered = 8;    // Number of fanotify marks registered
}
```

### AccessEvent

A file access event detected by fanotify.

```protobuf
message AccessEvent {
  google.protobuf.Timestamp timestamp = 1;  // When event occurred
  string guard_name = 2;         // JanusGuard resource name
  string namespace = 3;          // Kubernetes namespace
  string node_name = 4;          // Node where event occurred
  string pod_name = 5;           // Pod where event occurred
  string container_id = 6;       // Container where event occurred
  FanotifyEvent event_type = 7;  // Type of access event
  string path = 8;               // File path accessed
  AccessResponse response = 9;   // Action taken (allow/deny/audit)
  ProcessInfo process_info = 10; // Extended process information
  bool is_directory = 11;        // Whether path is a directory
  map<string, string> tags = 12; // Custom metadata from guard
  bool audit_logged = 13;        // Written to kernel audit log
}
```

### ProcessInfo

Process information with extended fields in V2.

```protobuf
message ProcessInfo {
  int32 pid = 1;                 // Process ID
  int32 tid = 2;                 // Thread ID
  int32 uid = 3;                 // User ID
  int32 gid = 4;                 // Group ID
  string comm = 5;               // Command name (max 16 chars)
  string exe = 6;                // Executable path
  int32 ppid = 7;                // Parent process ID (NEW in v2)
  repeated string cmdline = 8;   // Full command line (NEW in v2)
  string cwd = 9;                // Current working directory (NEW in v2)
}
```

### UpdateGuardRequest (NEW in v2)

Pause or resume an existing guard without recreating it.

```protobuf
enum UpdateAction {
  UPDATE_ACTION_UNSPECIFIED = 0;
  UPDATE_ACTION_PAUSE = 1;       // Stop monitoring but keep config
  UPDATE_ACTION_RESUME = 2;      // Resume a paused guard
}

message UpdateGuardRequest {
  string guard_name = 1;         // JanusGuard resource name
  string namespace = 2;          // Kubernetes namespace
  string pod_name = 3;           // Pod being guarded
  UpdateAction action = 4;       // Action to perform
}
```

### UpdatePolicyRequest (NEW in v2)

Update allow/deny patterns without recreating the guard.

```protobuf
message UpdatePolicyRequest {
  string guard_name = 1;         // JanusGuard resource name
  string namespace = 2;          // Kubernetes namespace
  string pod_name = 3;           // Pod being guarded
  repeated string deny_patterns = 4;   // New deny patterns (replaces existing)
  repeated string allow_patterns = 5;  // New allow patterns (replaces existing)
}

message UpdatePolicyResponse {
  string guard_id = 1;           // Guard identifier
  int32 deny_pattern_count = 2;  // Active deny patterns
  int32 allow_pattern_count = 3; // Active allow patterns
  bool cache_cleared = 4;        // Policy cache was cleared
}
```

## Event Types

| Enum Value | fanotify Flag | Description |
|------------|---------------|-------------|
| `FANOTIFY_EVENT_ACCESS` | FAN_ACCESS_PERM | Permission request for file read |
| `FANOTIFY_EVENT_OPEN` | FAN_OPEN_PERM | Permission request for file open |
| `FANOTIFY_EVENT_OPEN_EXEC` | FAN_OPEN_EXEC_PERM | Permission for execution |
| `FANOTIFY_EVENT_CLOSE_WRITE` | FAN_CLOSE_WRITE | File closed after writing |
| `FANOTIFY_EVENT_CLOSE` | FAN_CLOSE | File was closed |
| `FANOTIFY_EVENT_ALL` | All flags | Monitor all event types |

## Access Response Types

| Enum Value | Description |
|------------|-------------|
| `ACCESS_RESPONSE_ALLOW` | Access was allowed |
| `ACCESS_RESPONSE_DENY` | Access was denied |
| `ACCESS_RESPONSE_AUDIT` | Access allowed and audited |

## Policy Evaluation

When a file access request is received, janusd evaluates policies in order:

1. **Check deny patterns** - If path matches any deny pattern, return DENY
2. **Check allow patterns** - If path matches any allow pattern, return ALLOW
3. **Default behavior** - Use `default_response` from GuardSubject

### Glob Pattern Syntax

| Pattern | Description |
|---------|-------------|
| `*` | Match any sequence (not including `/`) |
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

### Go Client (Operator)

```go
import (
    pb "github.com/como-technologies/panoptes/gen/go/janus/v2"
    "google.golang.org/grpc"
)

conn, _ := grpc.Dial("localhost:50052", grpc.WithInsecure())
client := pb.NewJanusdServiceClient(conn)

// Create guard in enforce mode
resp, _ := client.CreateGuard(ctx, &pb.CreateGuardRequest{
    GuardName:   "default/my-guard",
    Namespace:   "default",
    NodeName:    "node-1",
    PodName:     "my-app-abc123",
    ContainerIds: []string{"containerd://xyz"},
    Enforcing:   true,
    Subjects: []*pb.GuardSubject{{
        Allow:  []string{"/app/**", "/tmp/**"},
        Deny:   []string{"/etc/shadow", "/root/**"},
        Events: []pb.FanotifyEvent{pb.FanotifyEvent_FANOTIFY_EVENT_OPEN},
        Audit:  true,
    }},
})

guardId := resp.GuardId

// Stream denied events
stream, _ := client.StreamAccessEvents(ctx, &pb.StreamAccessEventsRequest{
    GuardName:      "default/my-guard",
    IncludeAllowed: false,  // Only denied events
})

for {
    event, err := stream.Recv()
    if err == io.EOF {
        break
    }
    log.Printf("Denied: %s on %s (pid=%d, exe=%s)",
        event.EventType, event.Path,
        event.ProcessInfo.Pid, event.ProcessInfo.Exe)
}
```

### Update Policy Without Recreating Guard

```go
// Update policy live (NEW in v2)
_, err := client.UpdatePolicy(ctx, &pb.UpdatePolicyRequest{
    GuardName:     "default/my-guard",
    Namespace:     "default",
    PodName:       "my-app-abc123",
    AllowPatterns: []string{"/app/**", "/tmp/**", "/var/log/**"},
    DenyPatterns:  []string{"/etc/shadow"},
})
```

### Rust Client

```rust
use janus::v2::janusd_service_client::JanusdServiceClient;
use janus::v2::{CreateGuardRequest, GuardSubject, FanotifyEvent};

let mut client = JanusdServiceClient::connect("http://localhost:50052").await?;

let response = client.create_guard(CreateGuardRequest {
    guard_name: "default/my-guard".into(),
    namespace: "default".into(),
    node_name: "node-1".into(),
    pod_name: "my-app-abc123".into(),
    container_ids: vec!["containerd://xyz".into()],
    enforcing: true,
    subjects: vec![GuardSubject {
        allow: vec!["/app/**".into()],
        deny: vec!["/etc/shadow".into()],
        events: vec![FanotifyEvent::Open as i32],
        audit: true,
        ..Default::default()
    }],
    ..Default::default()
}).await?;

let guard_id = response.into_inner().guard_id;
```

## Error Handling

Errors are returned as standard gRPC status codes:

| Code | Description |
|------|-------------|
| `NOT_FOUND` | Guard or container not found |
| `ALREADY_EXISTS` | Guard already exists for pod |
| `RESOURCE_EXHAUSTED` | fanotify limit reached |
| `PERMISSION_DENIED` | Insufficient capabilities (need CAP_SYS_ADMIN) |
| `INVALID_ARGUMENT` | Invalid request parameters |

## Kernel Audit Integration

When `audit: true` is set in guard options, janusd writes structured records
to the kernel audit subsystem for each access event:

```
type=FANOTIFY msg=audit(1704883800.123:456):
  operation="open"
  path="/etc/shadow"
  response="deny"
  pid=12345
  ppid=1234
  uid=1000
  exe="/bin/cat"
  cmdline="cat /etc/shadow"
  guard="default/my-guard"
  namespace="default"
  pod="my-app-abc123"
```

## Migration from V1

If migrating from V1:

1. Update import path: `janus.v1` → `janus.v2`
2. Change client: `JanusServiceClient` → `JanusdServiceClient`
3. Use typed enums instead of string event types
4. Handle `google.protobuf.Timestamp` instead of int64 nanoseconds
5. Use `GuardSubject.allow`/`deny` instead of `GuardOptions.allow_patterns`/`deny_patterns`
6. Use `UpdatePolicy` RPC for live policy updates instead of recreating guards

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
