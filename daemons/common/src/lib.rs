//! # Panoptes Common Library
//!
//! Shared utilities for Panoptes file integrity monitoring and access auditing daemons.
//!
//! This crate provides:
//! - **Environment detection** - Detect deployment environment (host, container, WSL)
//! - **Path abstraction** - Consistent path resolution across environments
//! - **Capability verification** - Check required Linux capabilities at startup
//! - **Container runtime detection** - containerd, CRI-O support
//! - **Process information** - `/proc` filesystem access
//! - **Session management** - Generic daemon session handling
//! - **Event broadcasting** - Multi-client streaming support
//! - **Metrics collection** - Per-session and aggregate metrics
//! - **gRPC helpers** - Streaming utilities (with `grpc` feature)
//!
//! ## Architecture
//!
//! Daemon code should be free of environment-specific checks. All environment
//! awareness goes through the abstraction traits:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Daemon Code                            │
//! │  (argusd, janusd - pure business logic, no env checks)      │
//! └─────────────────────────────┬───────────────────────────────┘
//!                               │ uses traits
//! ┌─────────────────────────────▼───────────────────────────────┐
//! │                  panoptes-common                            │
//! │  ┌─────────────┐  ┌──────────────┐  ┌─────────────────┐    │
//! │  │ Environment │  │ PathProvider │  │ CapabilityCheck │    │
//! │  │   Trait     │  │    Trait     │  │     Trait       │    │
//! │  └─────────────┘  └──────────────┘  └─────────────────┘    │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Security Considerations
//!
//! The daemons using this library require elevated capabilities:
//! - `CAP_SYS_ADMIN` - Required for fanotify operations (janusd)
//! - `CAP_SYS_PTRACE` - Required for `/proc/{pid}/root` access
//! - `CAP_DAC_READ_SEARCH` - Optional for broader file access
//!
//! Use `CapabilityChecker` to verify capabilities at startup.
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::environment::{LinuxEnvironmentDetector, EnvironmentDetector, Feature};
//! use panoptes_common::capabilities::{LinuxCapabilityChecker, CapabilityChecker, ARGUSD_REQUIRED_CAPS};
//! use panoptes_common::container_runtime::detect_runtime;
//!
//! // Validate environment at startup
//! let env = LinuxEnvironmentDetector::new();
//! println!("Environment: {}", env.detect());
//!
//! for warning in env.environment_warnings() {
//!     println!("[{}] {}", warning.severity, warning.message);
//! }
//!
//! // Check capabilities
//! let caps = LinuxCapabilityChecker::new();
//! caps.require_all(ARGUSD_REQUIRED_CAPS)?;
//!
//! // Detect container runtime
//! if let Some(runtime) = detect_runtime() {
//!     println!("Detected runtime: {:?}", runtime.runtime_type());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

// Environment abstraction layer
pub mod capabilities;
pub mod environment;
pub mod paths;

// eBPF support detection (always compiled - for runtime auto-detection)
pub mod ebpf_support;

// eBPF loader/types (optional, for kernel-level process attribution)
#[cfg(feature = "ebpf")]
pub mod ebpf;

// Core modules
pub mod container_runtime;
pub mod error;
pub mod glog;
pub mod proc;
pub mod resource_limits;

// Daemon infrastructure modules
pub mod broadcast;
pub mod metrics;
pub mod session;

// gRPC helpers (optional, requires tonic)
pub mod grpc;

// Re-export environment abstraction types
pub use capabilities::{
    missing_capabilities_message, CapabilityChecker, CapabilityError, LinuxCapabilityChecker,
    RequiredCapability, ARGUSD_REQUIRED_CAPS, ARGUSD_REQUIRED_CAPS_EBPF, JANUSD_OPTIONAL_CAPS,
    JANUSD_REQUIRED_CAPS, JANUSD_REQUIRED_CAPS_EBPF,
};
pub use environment::{
    DeploymentEnvironment, EnvironmentDetector, EnvironmentError, EnvironmentWarning, Feature,
    LinuxEnvironmentDetector, WarningSeverity,
};
pub use paths::{LinuxPathProvider, PathProvider, RuntimeType as PathRuntimeType};

// Re-export commonly used types from core modules
pub use container_runtime::{
    detect_runtime, detect_runtime_type, runtime_for_container, ContainerRuntime,
    ContainerdRuntime, CriORuntime, RuntimeType,
};
pub use error::{CommonError, ProcError, RuntimeError, SecurityError};
pub use glog::GlogLayer;
pub use proc::{ProcessInfo, ProcessResolver, ProcfsProcessResolver};

// Re-export daemon infrastructure types
pub use broadcast::{EventBroadcaster, Filterable, StreamFilter};
pub use metrics::{
    AggregateTotals, BasicMetrics, DaemonMetrics, MetricsAggregator, MetricsSnapshot,
};
pub use session::{
    new_session_map, Session, SessionError, SessionManager, SessionMap, SessionState,
};

// Re-export gRPC helpers (when feature enabled)
#[cfg(feature = "grpc")]
pub use grpc::{broadcast_stream, filtered_broadcast_stream, stream_from_iter, GrpcStream};

// Always export basic stream helpers
pub use grpc::{basic_filtered_broadcast_stream, basic_stream_from_iter, BasicStream};

// Re-export eBPF support detection (for runtime mode selection)
pub use ebpf_support::{is_bpf_lsm_enabled, is_ebpf_supported};

// Re-export resource limit checking utilities
pub use resource_limits::{
    check_fd_limit, read_fanotify_limits, read_inotify_limits, ResourceLimitError,
    ResourceLimitsInfo,
};
