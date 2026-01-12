# Argus gRPC API v1

**Protocol Buffer definitions for the Argus file integrity monitoring daemon**

## Overview

This package defines the gRPC API for communication between the Argus operator
and argusd daemons. The API enables creating file watches, streaming events,
and managing watch lifecycle.

## Services

### ArgusService

The main service for file integrity monitoring operations.

```protobuf
service ArgusService {
  // Create a new file watch on container filesystem
  rpc CreateWatch(CreateWatchRequest) returns (CreateWatchResponse);

  // Destroy an existing watch
  rpc DestroyWatch(DestroyWatchRequest) returns (DestroyWatchResponse);

  // Get status of a specific watch
  rpc GetWatchStatus(GetWatchStatusRequest) returns (GetWatchStatusResponse);

  // List all active watches
  rpc ListWatches(ListWatchesRequest) returns (ListWatchesResponse);

  // Stream file events in real-time
  rpc StreamEvents(StreamEventsRequest) returns (stream FileEvent);
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

### CreateWatchRequest

Request to create a new file watch.

```protobuf
message CreateWatchRequest {
  // Watcher resource name (e.g., "default/my-watcher")
  string watcher_name = 1;

  // Kubernetes namespace
  string namespace = 2;

  // Pod name being watched
  string pod_name = 3;

  // Container ID (e.g., "containerd://abc123")
  string container_id = 4;

  // Paths to watch
  repeated string paths = 5;

  // Events to monitor (access, modify, create, delete, etc.)
  repeated string events = 6;

  // Watch options
  WatchOptions options = 7;
}

message WatchOptions {
  // Watch subdirectories recursively
  bool recursive = 1;

  // Maximum recursion depth (-1 for unlimited)
  int32 max_depth = 2;

  // Glob patterns to ignore
  repeated string ignore = 3;

  // Only watch directory events
  bool only_dir = 4;

  // Follow files when moved
  bool follow_move = 5;

  // Custom metadata tags
  map<string, string> tags = 6;
}
```

### CreateWatchResponse

```protobuf
message CreateWatchResponse {
  // Unique session ID for this watch
  int64 session_id = 1;

  // Number of inotify watches created
  int32 watch_count = 2;

  // Error message (empty on success)
  string error = 3;
}
```

### FileEvent

A file system event detected by inotify.

```protobuf
message FileEvent {
  // Session this event belongs to
  int64 session_id = 1;

  // Event type (access, modify, create, delete, etc.)
  string event_type = 2;

  // Full path to the file
  string path = 3;

  // Filename only
  string filename = 4;

  // Whether path is a directory
  bool is_dir = 5;

  // Cookie for move events (links MOVED_FROM and MOVED_TO)
  uint32 cookie = 6;

  // Timestamp (Unix nanoseconds)
  int64 timestamp = 7;

  // Metadata from watch configuration
  map<string, string> tags = 8;

  // Watcher resource name
  string watcher_name = 9;

  // Pod name
  string pod_name = 10;

  // Container ID
  string container_id = 11;
}
```

### StreamEventsRequest

```protobuf
message StreamEventsRequest {
  // Filter by session ID (0 for all)
  int64 session_id = 1;

  // Filter by watcher name (empty for all)
  string watcher_name = 2;

  // Filter by pod name (empty for all)
  string pod_name = 3;

  // Filter by event types (empty for all)
  repeated string event_types = 4;
}
```

## Event Types

| Event Type | inotify Flag | Description |
|------------|--------------|-------------|
| `access` | IN_ACCESS | File was accessed (read) |
| `attrib` | IN_ATTRIB | Metadata changed (permissions, timestamps) |
| `closewrite` | IN_CLOSE_WRITE | File closed after writing |
| `closenowrite` | IN_CLOSE_NOWRITE | File closed without writing |
| `create` | IN_CREATE | File/directory created in watched dir |
| `delete` | IN_DELETE | File/directory deleted from watched dir |
| `deleteself` | IN_DELETE_SELF | Watched file/directory deleted |
| `modify` | IN_MODIFY | File was modified |
| `moveself` | IN_MOVE_SELF | Watched file/directory moved |
| `movedfrom` | IN_MOVED_FROM | File moved out of watched directory |
| `movedto` | IN_MOVED_TO | File moved into watched directory |
| `open` | IN_OPEN | File was opened |

## Usage Examples

### Go Client

```go
import (
    pb "github.com/como-technologies/panoptes/proto/argus/v1"
    "google.golang.org/grpc"
)

conn, _ := grpc.Dial("localhost:50051", grpc.WithInsecure())
client := pb.NewArgusServiceClient(conn)

// Create watch
resp, _ := client.CreateWatch(ctx, &pb.CreateWatchRequest{
    WatcherName: "default/my-watcher",
    Namespace:   "default",
    PodName:     "my-app-abc123",
    ContainerId: "containerd://xyz",
    Paths:       []string{"/etc/passwd", "/app/config/"},
    Events:      []string{"modify", "delete"},
    Options: &pb.WatchOptions{
        Recursive: true,
        Ignore:    []string{"*.tmp"},
    },
})

sessionId := resp.SessionId

// Stream events
stream, _ := client.StreamEvents(ctx, &pb.StreamEventsRequest{
    SessionId: sessionId,
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
use argus::v1::argus_service_client::ArgusServiceClient;

let mut client = ArgusServiceClient::connect("http://localhost:50051").await?;

let response = client.create_watch(CreateWatchRequest {
    watcher_name: "default/my-watcher".into(),
    namespace: "default".into(),
    pod_name: "my-app-abc123".into(),
    container_id: "containerd://xyz".into(),
    paths: vec!["/etc/passwd".into()],
    events: vec!["modify".into()],
    options: Some(WatchOptions {
        recursive: true,
        ..Default::default()
    }),
}).await?;

let session_id = response.into_inner().session_id;
```

## Error Handling

All responses include an `error` field that is empty on success:

```protobuf
message DestroyWatchResponse {
  bool success = 1;
  string error = 2;  // Empty on success
}
```

Common error messages:
- `"session not found"` - Invalid session ID
- `"path not found"` - Container filesystem path doesn't exist
- `"max watches exceeded"` - inotify limit reached
- `"permission denied"` - Insufficient capabilities

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
