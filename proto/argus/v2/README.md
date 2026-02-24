# Argus gRPC API v2

**Protocol Buffer definitions for the Argus file integrity monitoring daemon (Rust)**

## Overview

This package defines the gRPC API for the Rust implementation of the Argus daemon.
The API enables creating file watches, streaming events, and managing watch lifecycle.

**V2 is the current API** used by the Rust daemon implementation. V1 is the legacy
API from the original C daemon and is retained for reference only.

### Key Differences from V1

| Feature | V1 | V2 |
|---------|----|----|
| Implementation | C daemon | Rust daemon |
| Package | `argus.v1` | `argus.v2` |
| Service name | `ArgusService` | `ArgusdService` |
| Event types | String-based | Typed enum (`InotifyEvent`) |
| Timestamps | `int64` nanoseconds | `google.protobuf.Timestamp` |
| Process info | Not available | `ProcessInfo` (optional, for future eBPF) |
| Watch control | Create/Destroy only | `UpdateWatch` for pause/resume |

## Services

### ArgusdService

The main service for file integrity monitoring operations.

```protobuf
service ArgusdService {
  // Create a new file watch for the specified configuration
  rpc CreateWatch(CreateWatchRequest) returns (CreateWatchResponse);

  // Remove an existing file watch
  rpc DestroyWatch(DestroyWatchRequest) returns (google.protobuf.Empty);

  // Get current state of all watches on this node
  rpc GetWatchState(GetWatchStateRequest) returns (stream WatchState);

  // Stream file events in real-time
  rpc StreamEvents(StreamEventsRequest) returns (stream FileEvent);

  // Get metrics for monitoring
  rpc GetMetrics(GetMetricsRequest) returns (MetricsResponse);

  // Pause or resume an existing watch (NEW in v2)
  rpc UpdateWatch(UpdateWatchRequest) returns (UpdateWatchResponse);
}
```

## Messages

### CreateWatchRequest

Request to create a new file watch.

```protobuf
message CreateWatchRequest {
  string watcher_name = 1;       // ArgusWatcher resource name
  string namespace = 2;          // Kubernetes namespace
  string node_name = 3;          // Kubernetes node name
  string pod_name = 4;           // Pod being watched
  repeated string container_ids = 5;  // Container IDs to watch
  repeated int32 pids = 6;       // Process IDs (from containers)
  repeated WatchSubject subjects = 7; // What paths/events to watch
  string log_format = 8;         // Custom log format template
  bool paused = 9;               // Start in paused state
}

message WatchSubject {
  repeated string paths = 1;     // File/directory paths to monitor
  repeated InotifyEvent events = 2;  // Event types to watch
  repeated string ignore = 3;    // Glob patterns to ignore
  bool recursive = 4;            // Watch subdirectories
  int32 max_depth = 5;           // Recursion depth limit (0 = unlimited)
  bool only_dir = 6;             // Only watch directories
  bool follow_move = 7;          // Track moved files by inode
  map<string, string> tags = 8;  // Custom metadata
  bool skip_if_missing = 9;      // Skip non-existent paths (default: false = proxy watch)
}
```

### CreateWatchResponse

```protobuf
message CreateWatchResponse {
  string watch_id = 1;           // Unique watch identifier
  string node_name = 2;          // Node where watch was created
  string pod_name = 3;           // Pod being watched
  int32 watched_paths = 4;       // Number of paths being watched
  bool paused = 5;               // Whether watch is paused
  bool watches_ready = 6;        // All inotify watches registered
}
```

### FileEvent

A file system event detected by inotify.

```protobuf
message FileEvent {
  google.protobuf.Timestamp timestamp = 1;  // When event occurred
  string watcher_name = 2;       // ArgusWatcher resource name
  string namespace = 3;          // Kubernetes namespace
  string node_name = 4;          // Node where event occurred
  string pod_name = 5;           // Pod where event occurred
  string container_id = 6;       // Container where event occurred
  InotifyEvent event_type = 7;   // Type of filesystem event
  string path = 8;               // File path that triggered event
  string filename = 9;           // Filename (for directory events)
  bool is_directory = 10;        // Whether path is a directory
  uint64 inode = 11;             // Inode number
  map<string, string> tags = 12; // Custom metadata from watch
  optional ProcessInfo process_info = 13;  // Process info (NEW in v2)
}
```

### ProcessInfo (NEW in v2)

Process information for correlating events to processes. Note: For inotify-based
monitoring, this will be empty until eBPF/audit integration is implemented.

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

### UpdateWatchRequest (NEW in v2)

Pause or resume an existing watch without recreating it.

```protobuf
enum UpdateAction {
  UPDATE_ACTION_UNSPECIFIED = 0;
  UPDATE_ACTION_PAUSE = 1;       // Stop monitoring but keep config
  UPDATE_ACTION_RESUME = 2;      // Resume a paused watch
}

message UpdateWatchRequest {
  string watcher_name = 1;       // ArgusWatcher resource name
  string namespace = 2;          // Kubernetes namespace
  string pod_name = 3;           // Pod being watched
  UpdateAction action = 4;       // Action to perform
}
```

## Event Types

| Enum Value | inotify Flag | Description |
|------------|--------------|-------------|
| `INOTIFY_EVENT_ACCESS` | IN_ACCESS | File was accessed (read) |
| `INOTIFY_EVENT_ATTRIB` | IN_ATTRIB | Metadata changed |
| `INOTIFY_EVENT_CLOSE_WRITE` | IN_CLOSE_WRITE | File closed after writing |
| `INOTIFY_EVENT_CLOSE_NOWRITE` | IN_CLOSE_NOWRITE | File closed without writing |
| `INOTIFY_EVENT_CREATE` | IN_CREATE | File/directory created |
| `INOTIFY_EVENT_DELETE` | IN_DELETE | File/directory deleted |
| `INOTIFY_EVENT_DELETE_SELF` | IN_DELETE_SELF | Watched item deleted |
| `INOTIFY_EVENT_MODIFY` | IN_MODIFY | File was modified |
| `INOTIFY_EVENT_MOVE_SELF` | IN_MOVE_SELF | Watched item moved |
| `INOTIFY_EVENT_MOVED_FROM` | IN_MOVED_FROM | File moved out of directory |
| `INOTIFY_EVENT_MOVED_TO` | IN_MOVED_TO | File moved into directory |
| `INOTIFY_EVENT_OPEN` | IN_OPEN | File was opened |
| `INOTIFY_EVENT_ALL` | All flags | Monitor all event types |

## Usage Examples

### Go Client (Operator)

```go
import (
    pb "github.com/como-technologies/panoptes/gen/go/argus/v2"
    "google.golang.org/grpc"
)

conn, _ := grpc.Dial("localhost:50051", grpc.WithInsecure())
client := pb.NewArgusdServiceClient(conn)

// Create watch
resp, _ := client.CreateWatch(ctx, &pb.CreateWatchRequest{
    WatcherName: "default/my-watcher",
    Namespace:   "default",
    NodeName:    "node-1",
    PodName:     "my-app-abc123",
    ContainerIds: []string{"containerd://xyz"},
    Subjects: []*pb.WatchSubject{{
        Paths:     []string{"/etc/passwd", "/app/config/"},
        Events:    []pb.InotifyEvent{pb.InotifyEvent_INOTIFY_EVENT_MODIFY},
        Recursive: true,
        Ignore:    []string{"*.tmp"},
    }},
})

watchId := resp.WatchId

// Stream events
stream, _ := client.StreamEvents(ctx, &pb.StreamEventsRequest{
    WatcherName: "default/my-watcher",
})

for {
    event, err := stream.Recv()
    if err == io.EOF {
        break
    }
    log.Printf("Event: %s on %s", event.EventType, event.Path)
}
```

### Rust Client

```rust
use argus::v2::argusd_service_client::ArgusdServiceClient;
use argus::v2::{CreateWatchRequest, WatchSubject, InotifyEvent};

let mut client = ArgusdServiceClient::connect("http://localhost:50051").await?;

let response = client.create_watch(CreateWatchRequest {
    watcher_name: "default/my-watcher".into(),
    namespace: "default".into(),
    node_name: "node-1".into(),
    pod_name: "my-app-abc123".into(),
    container_ids: vec!["containerd://xyz".into()],
    subjects: vec![WatchSubject {
        paths: vec!["/etc/passwd".into()],
        events: vec![InotifyEvent::Modify as i32],
        recursive: true,
        ..Default::default()
    }],
    ..Default::default()
}).await?;

let watch_id = response.into_inner().watch_id;
```

## Error Handling

Errors are returned as standard gRPC status codes:

| Code | Description |
|------|-------------|
| `NOT_FOUND` | Watch or container not found |
| `ALREADY_EXISTS` | Watch already exists for pod |
| `RESOURCE_EXHAUSTED` | inotify limit reached |
| `PERMISSION_DENIED` | Insufficient capabilities |
| `INVALID_ARGUMENT` | Invalid request parameters |

## Migration from V1

If migrating from V1:

1. Update import path: `argus.v1` → `argus.v2`
2. Change client: `ArgusServiceClient` → `ArgusdServiceClient`
3. Use typed enums instead of string event types
4. Handle `google.protobuf.Timestamp` instead of int64 nanoseconds
5. Use `WatchSubject` instead of `WatchOptions`

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
