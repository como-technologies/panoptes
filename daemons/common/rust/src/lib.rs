//! # Panoptes Common Library
//!
//! Shared utilities for Panoptes file integrity monitoring and access auditing daemons.
//!
//! This crate provides:
//! - Container runtime detection and PID resolution (containerd, CRI-O)
//! - Process information retrieval from `/proc` filesystem
//! - Generic session management for daemon sessions
//! - Event broadcasting for multi-client streaming
//! - Metrics collection and aggregation
//! - gRPC streaming helpers (with `grpc` feature)
//! - Common error types and security utilities
//!
//! ## Security Considerations
//!
//! The daemons using this library require elevated capabilities:
//! - `CAP_SYS_ADMIN` - Required for fanotify operations
//! - `CAP_SYS_PTRACE` - Required for `/proc/{pid}/root` access
//! - `CAP_DAC_READ_SEARCH` - Required for accessing files without permission
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::container_runtime::{detect_runtime, ContainerRuntime};
//! use panoptes_common::proc::ProcessResolver;
//!
//! // Detect container runtime
//! if let Some(runtime) = detect_runtime() {
//!     println!("Detected runtime: {:?}", runtime.runtime_type());
//!
//!     // Resolve container PID
//!     let pid = runtime.resolve_pid("abc123def456")?;
//!     let root = runtime.resolve_container_root(pid);
//!     println!("Container root: {}", root.display());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

// Core modules
pub mod container_runtime;
pub mod error;
pub mod glog;
pub mod proc;

// Daemon infrastructure modules
pub mod broadcast;
pub mod metrics;
pub mod session;

// gRPC helpers (optional, requires tonic)
pub mod grpc;

// Re-export commonly used types from core modules
pub use container_runtime::{
    ContainerRuntime, ContainerdRuntime, CriORuntime, RuntimeType,
    detect_runtime, detect_runtime_type, runtime_for_container,
};
pub use error::{CommonError, RuntimeError, ProcError, SecurityError};
pub use glog::GlogLayer;
pub use proc::{ProcessInfo, ProcessResolver, ProcfsProcessResolver};

// Re-export daemon infrastructure types
pub use broadcast::{EventBroadcaster, Filterable, StreamFilter};
pub use metrics::{AggregateTotals, BasicMetrics, DaemonMetrics, MetricsAggregator, MetricsSnapshot};
pub use session::{new_session_map, Session, SessionError, SessionManager, SessionMap, SessionState};

// Re-export gRPC helpers (when feature enabled)
#[cfg(feature = "grpc")]
pub use grpc::{broadcast_stream, filtered_broadcast_stream, stream_from_iter, GrpcStream};

// Always export basic stream helpers
pub use grpc::{basic_filtered_broadcast_stream, basic_stream_from_iter, BasicStream};
