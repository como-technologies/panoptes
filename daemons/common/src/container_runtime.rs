//! # Container Runtime Detection
//!
//! This module provides abstractions for detecting and interacting with
//! container runtimes (containerd, CRI-O) in Kubernetes environments.
//!
//! ## Supported Runtimes
//!
//! | Runtime | Socket Path | PID File Pattern |
//! |---------|-------------|------------------|
//! | containerd | `/run/containerd/containerd.sock` | `/run/containerd/io.containerd.runtime.v2.task/k8s.io/{id}/init.pid` |
//! | CRI-O | `/var/run/crio/crio.sock` | `/var/run/crio/{id}/pidfile` |
//!
//! ## Container ID Format
//!
//! Container IDs in Kubernetes include a runtime prefix:
//! - `containerd://abc123def456...` - containerd runtime
//! - `cri-o://abc123def456...` - CRI-O runtime
//!
//! ## Security Considerations
//!
//! - Requires `CAP_SYS_PTRACE` for `/proc/{pid}/root` access
//! - PID files are read from trusted paths only
//! - Container IDs are validated before use
//! - Paths are validated to prevent traversal attacks
//!
//! ## Environment Variables
//!
//! - `HOST_ROOT_PATH` - Override host root path (default: auto-detect `/host` or `/`)
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::container_runtime::{detect_runtime, ContainerRuntime};
//!
//! // Auto-detect and use runtime
//! if let Some(runtime) = detect_runtime() {
//!     println!("Detected: {:?}", runtime.runtime_type());
//!
//!     // Resolve container PID
//!     let pid = runtime.resolve_pid("containerd://abc123")?;
//!     println!("Container PID: {}", pid);
//!
//!     // Get container root filesystem path
//!     let root = runtime.resolve_container_root(pid);
//!     println!("Container root: {}", root.display());
//! }
//! # Ok::<(), panoptes_common::error::RuntimeError>(())
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::RuntimeError;

/// Type of container runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeType {
    /// containerd runtime (default for most Kubernetes distributions).
    Containerd,
    /// CRI-O runtime (used by OpenShift and some distributions).
    CriO,
}

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeType::Containerd => write!(f, "containerd"),
            RuntimeType::CriO => write!(f, "CRI-O"),
        }
    }
}

/// Trait for container runtime operations.
///
/// This trait abstracts container runtime interactions, allowing the daemon
/// to work with different runtimes (containerd, CRI-O) uniformly.
///
/// # Implementors
///
/// - [`ContainerdRuntime`] - containerd runtime support
/// - [`CriORuntime`] - CRI-O runtime support
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to allow use across async tasks.
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::container_runtime::{ContainerRuntime, RuntimeType};
///
/// fn process_container(runtime: &dyn ContainerRuntime, container_id: &str) {
///     match runtime.resolve_pid(container_id) {
///         Ok(pid) => {
///             let root = runtime.resolve_container_root(pid);
///             println!("Container {} (PID {}) root: {}", container_id, pid, root.display());
///         }
///         Err(e) => eprintln!("Failed to resolve container: {}", e),
///     }
/// }
/// ```
pub trait ContainerRuntime: Send + Sync {
    /// Returns the type of this runtime.
    fn runtime_type(&self) -> RuntimeType;

    /// Resolves a container ID to its init process PID.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Full container ID with runtime prefix
    ///   (e.g., `containerd://abc123...` or `cri-o://abc123...`)
    ///
    /// # Returns
    ///
    /// The PID of the container's init process (PID 1 inside the container).
    ///
    /// # Errors
    ///
    /// - [`RuntimeError::InvalidContainerId`] - ID format is invalid
    /// - [`RuntimeError::PidFileNotFound`] - Container doesn't exist
    /// - [`RuntimeError::PidFileRead`] - I/O error reading PID file
    /// - [`RuntimeError::InvalidPidContent`] - PID file contains non-integer
    ///
    /// # Security
    ///
    /// Only reads from trusted runtime paths. Container IDs are validated
    /// to prevent path traversal attacks.
    fn resolve_pid(&self, container_id: &str) -> Result<u32, RuntimeError>;

    /// Returns the path to a container's root filesystem.
    ///
    /// Uses `/proc/{pid}/root` which provides access to the container's
    /// root filesystem namespace.
    ///
    /// # Arguments
    ///
    /// * `pid` - PID of the container's init process
    ///
    /// # Returns
    ///
    /// Path to the container's root filesystem (e.g., `/proc/12345/root`).
    ///
    /// # Security
    ///
    /// Accessing this path requires `CAP_SYS_PTRACE` capability.
    fn resolve_container_root(&self, pid: u32) -> PathBuf;

    /// Resolves a path inside the container to its host path.
    ///
    /// # Arguments
    ///
    /// * `pid` - PID of the container's init process
    /// * `container_path` - Path inside the container (e.g., `/etc/passwd`)
    ///
    /// # Returns
    ///
    /// Full host path (e.g., `/proc/12345/root/etc/passwd`).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use panoptes_common::container_runtime::{ContainerRuntime, ContainerdRuntime};
    /// # let runtime = ContainerdRuntime::new();
    /// let host_path = runtime.resolve_container_path(12345, "/etc/passwd");
    /// assert_eq!(host_path.to_str().unwrap(), "/proc/12345/root/etc/passwd");
    /// ```
    fn resolve_container_path(&self, pid: u32, container_path: &str) -> PathBuf {
        let container_path = container_path.trim_start_matches('/');
        self.resolve_container_root(pid).join(container_path)
    }

    /// Extracts the container ID without the runtime prefix.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Full container ID with prefix
    ///
    /// # Returns
    ///
    /// Container ID without the runtime prefix.
    ///
    /// # Errors
    ///
    /// - [`RuntimeError::InvalidContainerId`] - ID doesn't have expected prefix
    fn strip_prefix<'a>(&self, container_id: &'a str) -> Result<&'a str, RuntimeError>;
}

/// containerd runtime implementation.
///
/// # Paths
///
/// - Socket: `/run/containerd/containerd.sock`
/// - PID files: `/run/containerd/io.containerd.runtime.v2.task/k8s.io/{id}/init.pid`
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::container_runtime::{ContainerdRuntime, ContainerRuntime};
///
/// let runtime = ContainerdRuntime::new();
/// let pid = runtime.resolve_pid("containerd://abc123def456")?;
/// # Ok::<(), panoptes_common::error::RuntimeError>(())
/// ```
#[derive(Debug, Clone)]
pub struct ContainerdRuntime {
    /// Base path for runtime files (usually `/` or `/host`).
    host_root: PathBuf,
}

impl ContainerdRuntime {
    /// Default socket path for containerd.
    pub const SOCKET_PATH: &'static str = "/run/containerd/containerd.sock";

    /// Container ID prefix for containerd.
    pub const PREFIX: &'static str = "containerd://";

    /// PID file path template.
    /// `{0}` = host root, `{1}` = container ID
    const PID_PATH_TEMPLATE: &'static str =
        "{0}/run/containerd/io.containerd.runtime.v2.task/k8s.io/{1}/init.pid";

    /// Creates a new containerd runtime with auto-detected host root.
    ///
    /// Checks `HOST_ROOT_PATH` environment variable first, then falls back
    /// to `/host` if it exists, otherwise `/`.
    pub fn new() -> Self {
        Self {
            host_root: detect_host_root(),
        }
    }

    /// Creates a new containerd runtime with explicit host root.
    ///
    /// # Arguments
    ///
    /// * `host_root` - Path to host root (e.g., `/host` when running in container)
    pub fn with_host_root(host_root: PathBuf) -> Self {
        Self { host_root }
    }

    /// Returns the path to the PID file for a container.
    fn pid_file_path(&self, container_id: &str) -> PathBuf {
        let path = Self::PID_PATH_TEMPLATE
            .replace("{0}", self.host_root.to_str().unwrap_or(""))
            .replace("{1}", container_id);
        PathBuf::from(path)
    }

    /// Checks if containerd socket exists (runtime is available).
    pub fn is_available(&self) -> bool {
        let socket_path = self.host_root.join(Self::SOCKET_PATH.trim_start_matches('/'));
        socket_path.exists()
    }
}

impl Default for ContainerdRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRuntime for ContainerdRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Containerd
    }

    fn resolve_pid(&self, container_id: &str) -> Result<u32, RuntimeError> {
        let id = self.strip_prefix(container_id)?;

        // Validate container ID (alphanumeric only, prevent path traversal)
        validate_container_id(id)?;

        let pid_path = self.pid_file_path(id);

        // Read PID from file
        let content = fs::read_to_string(&pid_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RuntimeError::PidFileNotFound {
                    container_id: container_id.to_string(),
                    path: pid_path.clone(),
                }
            } else {
                RuntimeError::PidFileRead {
                    path: pid_path.clone(),
                    source: e,
                }
            }
        })?;

        // Parse PID
        content
            .trim()
            .parse::<u32>()
            .map_err(|_| RuntimeError::InvalidPidContent {
                path: pid_path,
                content: content.trim().to_string(),
            })
    }

    fn resolve_container_root(&self, pid: u32) -> PathBuf {
        PathBuf::from(format!("/proc/{}/root", pid))
    }

    fn strip_prefix<'a>(&self, container_id: &'a str) -> Result<&'a str, RuntimeError> {
        container_id
            .strip_prefix(Self::PREFIX)
            .ok_or_else(|| RuntimeError::InvalidContainerId {
                id: container_id.to_string(),
                reason: format!("expected prefix '{}'", Self::PREFIX),
            })
    }
}

/// CRI-O runtime implementation.
///
/// # Paths
///
/// - Socket: `/var/run/crio/crio.sock`
/// - PID files: `/var/run/crio/{id}/pidfile`
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::container_runtime::{CriORuntime, ContainerRuntime};
///
/// let runtime = CriORuntime::new();
/// let pid = runtime.resolve_pid("cri-o://abc123def456")?;
/// # Ok::<(), panoptes_common::error::RuntimeError>(())
/// ```
#[derive(Debug, Clone)]
pub struct CriORuntime {
    /// Base path for runtime files (usually `/` or `/host`).
    host_root: PathBuf,
}

impl CriORuntime {
    /// Default socket path for CRI-O.
    pub const SOCKET_PATH: &'static str = "/var/run/crio/crio.sock";

    /// Container ID prefix for CRI-O.
    pub const PREFIX: &'static str = "cri-o://";

    /// PID file path template.
    const PID_PATH_TEMPLATE: &'static str = "{0}/var/run/crio/{1}/pidfile";

    /// Creates a new CRI-O runtime with auto-detected host root.
    pub fn new() -> Self {
        Self {
            host_root: detect_host_root(),
        }
    }

    /// Creates a new CRI-O runtime with explicit host root.
    pub fn with_host_root(host_root: PathBuf) -> Self {
        Self { host_root }
    }

    /// Returns the path to the PID file for a container.
    fn pid_file_path(&self, container_id: &str) -> PathBuf {
        let path = Self::PID_PATH_TEMPLATE
            .replace("{0}", self.host_root.to_str().unwrap_or(""))
            .replace("{1}", container_id);
        PathBuf::from(path)
    }

    /// Checks if CRI-O socket exists (runtime is available).
    pub fn is_available(&self) -> bool {
        let socket_path = self.host_root.join(Self::SOCKET_PATH.trim_start_matches('/'));
        socket_path.exists()
    }
}

impl Default for CriORuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRuntime for CriORuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::CriO
    }

    fn resolve_pid(&self, container_id: &str) -> Result<u32, RuntimeError> {
        let id = self.strip_prefix(container_id)?;

        // Validate container ID
        validate_container_id(id)?;

        let pid_path = self.pid_file_path(id);

        // Read PID from file
        let content = fs::read_to_string(&pid_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RuntimeError::PidFileNotFound {
                    container_id: container_id.to_string(),
                    path: pid_path.clone(),
                }
            } else {
                RuntimeError::PidFileRead {
                    path: pid_path.clone(),
                    source: e,
                }
            }
        })?;

        // Parse PID
        content
            .trim()
            .parse::<u32>()
            .map_err(|_| RuntimeError::InvalidPidContent {
                path: pid_path,
                content: content.trim().to_string(),
            })
    }

    fn resolve_container_root(&self, pid: u32) -> PathBuf {
        PathBuf::from(format!("/proc/{}/root", pid))
    }

    fn strip_prefix<'a>(&self, container_id: &'a str) -> Result<&'a str, RuntimeError> {
        container_id
            .strip_prefix(Self::PREFIX)
            .ok_or_else(|| RuntimeError::InvalidContainerId {
                id: container_id.to_string(),
                reason: format!("expected prefix '{}'", Self::PREFIX),
            })
    }
}

/// Detects the container runtime from a container ID.
///
/// Examines the prefix of the container ID to determine which runtime
/// manages the container.
///
/// # Arguments
///
/// * `container_id` - Container ID with runtime prefix
///
/// # Returns
///
/// The detected [`RuntimeType`], or `None` if unrecognized.
///
/// # Example
///
/// ```rust
/// use panoptes_common::container_runtime::{detect_runtime_type, RuntimeType};
///
/// assert_eq!(
///     detect_runtime_type("containerd://abc123"),
///     Some(RuntimeType::Containerd)
/// );
/// assert_eq!(
///     detect_runtime_type("cri-o://abc123"),
///     Some(RuntimeType::CriO)
/// );
/// assert_eq!(
///     detect_runtime_type("docker://abc123"),
///     None
/// );
/// ```
pub fn detect_runtime_type(container_id: &str) -> Option<RuntimeType> {
    if container_id.starts_with(ContainerdRuntime::PREFIX) {
        Some(RuntimeType::Containerd)
    } else if container_id.starts_with(CriORuntime::PREFIX) {
        Some(RuntimeType::CriO)
    } else {
        None
    }
}

/// Auto-detects the available container runtime.
///
/// Checks for runtime sockets in this order:
/// 1. containerd (`/run/containerd/containerd.sock`)
/// 2. CRI-O (`/var/run/crio/crio.sock`)
///
/// # Returns
///
/// A boxed trait object for the detected runtime, or `None` if no
/// supported runtime is found.
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::container_runtime::detect_runtime;
///
/// match detect_runtime() {
///     Some(runtime) => println!("Found runtime: {}", runtime.runtime_type()),
///     None => eprintln!("No supported container runtime found"),
/// }
/// ```
pub fn detect_runtime() -> Option<Box<dyn ContainerRuntime>> {
    let containerd = ContainerdRuntime::new();
    if containerd.is_available() {
        return Some(Box::new(containerd));
    }

    let crio = CriORuntime::new();
    if crio.is_available() {
        return Some(Box::new(crio));
    }

    None
}

/// Creates a runtime instance for a specific container ID.
///
/// Detects the runtime type from the container ID prefix and returns
/// an appropriate runtime instance.
///
/// # Arguments
///
/// * `container_id` - Container ID with runtime prefix
///
/// # Returns
///
/// A boxed trait object for the appropriate runtime.
///
/// # Errors
///
/// - [`RuntimeError::UnknownRuntime`] - Container ID has unrecognized prefix
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::container_runtime::runtime_for_container;
///
/// let runtime = runtime_for_container("containerd://abc123")?;
/// let pid = runtime.resolve_pid("containerd://abc123")?;
/// # Ok::<(), panoptes_common::error::RuntimeError>(())
/// ```
pub fn runtime_for_container(container_id: &str) -> Result<Box<dyn ContainerRuntime>, RuntimeError> {
    match detect_runtime_type(container_id) {
        Some(RuntimeType::Containerd) => Ok(Box::new(ContainerdRuntime::new())),
        Some(RuntimeType::CriO) => Ok(Box::new(CriORuntime::new())),
        None => Err(RuntimeError::UnknownRuntime {
            id: container_id.to_string(),
        }),
    }
}

/// Detects the host root path.
///
/// Checks in order:
/// 1. `HOST_ROOT_PATH` environment variable
/// 2. `/host` directory (common when running in Kubernetes)
/// 3. `/` (default)
fn detect_host_root() -> PathBuf {
    // Check environment variable first
    if let Ok(path) = std::env::var("HOST_ROOT_PATH") {
        return PathBuf::from(path);
    }

    // Check if /host exists (running in container with host mount)
    let host_path = Path::new("/host");
    if host_path.exists() && host_path.is_dir() {
        return host_path.to_path_buf();
    }

    // Default to root
    PathBuf::from("/")
}

/// Validates a container ID to prevent path traversal attacks.
///
/// # Security
///
/// Container IDs should only contain:
/// - Alphanumeric characters (a-z, A-Z, 0-9)
/// - Hyphens (-)
/// - Underscores (_)
///
/// Specifically rejects:
/// - Path separators (`/`, `\`)
/// - Parent directory references (`..`)
/// - Null bytes
fn validate_container_id(id: &str) -> Result<(), RuntimeError> {
    if id.is_empty() {
        return Err(RuntimeError::InvalidContainerId {
            id: id.to_string(),
            reason: "container ID is empty".to_string(),
        });
    }

    // Check for path traversal attempts
    if id.contains("..") || id.contains('/') || id.contains('\\') || id.contains('\0') {
        return Err(RuntimeError::InvalidContainerId {
            id: id.to_string(),
            reason: "container ID contains invalid characters (potential path traversal)"
                .to_string(),
        });
    }

    // Validate characters (alphanumeric, hyphens, underscores only)
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(RuntimeError::InvalidContainerId {
            id: id.to_string(),
            reason: "container ID contains invalid characters".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod runtime_type {
        use super::*;

        #[test]
        fn test_display() {
            assert_eq!(RuntimeType::Containerd.to_string(), "containerd");
            assert_eq!(RuntimeType::CriO.to_string(), "CRI-O");
        }

        #[test]
        fn test_equality() {
            assert_eq!(RuntimeType::Containerd, RuntimeType::Containerd);
            assert_ne!(RuntimeType::Containerd, RuntimeType::CriO);
        }
    }

    mod detect_runtime_type {
        use super::*;

        #[test]
        fn test_containerd_prefix() {
            assert_eq!(
                detect_runtime_type("containerd://abc123def456"),
                Some(RuntimeType::Containerd)
            );
        }

        #[test]
        fn test_crio_prefix() {
            assert_eq!(
                detect_runtime_type("cri-o://abc123def456"),
                Some(RuntimeType::CriO)
            );
        }

        #[test]
        fn test_unknown_prefix() {
            assert_eq!(detect_runtime_type("docker://abc123"), None);
            assert_eq!(detect_runtime_type("unknown://abc123"), None);
            assert_eq!(detect_runtime_type("abc123"), None);
        }

        #[test]
        fn test_empty_string() {
            assert_eq!(detect_runtime_type(""), None);
        }
    }

    mod container_id_validation {
        use super::*;

        #[test]
        fn test_valid_ids() {
            assert!(validate_container_id("abc123").is_ok());
            assert!(validate_container_id("ABC123").is_ok());
            assert!(validate_container_id("abc-123").is_ok());
            assert!(validate_container_id("abc_123").is_ok());
            assert!(validate_container_id("a1b2c3d4e5f6").is_ok());
        }

        #[test]
        fn test_empty_id() {
            let result = validate_container_id("");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("empty"));
        }

        #[test]
        fn test_path_traversal() {
            assert!(validate_container_id("../../../etc/passwd").is_err());
            assert!(validate_container_id("abc/../def").is_err());
            assert!(validate_container_id("abc/def").is_err());
            assert!(validate_container_id("abc\\def").is_err());
        }

        #[test]
        fn test_null_byte() {
            assert!(validate_container_id("abc\0def").is_err());
        }

        #[test]
        fn test_invalid_characters() {
            assert!(validate_container_id("abc.def").is_err());
            assert!(validate_container_id("abc:def").is_err());
            assert!(validate_container_id("abc def").is_err());
        }
    }

    mod containerd_runtime {
        use super::*;

        #[test]
        fn test_strip_prefix_valid() {
            let runtime = ContainerdRuntime::new();
            assert_eq!(
                runtime.strip_prefix("containerd://abc123").unwrap(),
                "abc123"
            );
        }

        #[test]
        fn test_strip_prefix_invalid() {
            let runtime = ContainerdRuntime::new();
            assert!(runtime.strip_prefix("cri-o://abc123").is_err());
            assert!(runtime.strip_prefix("abc123").is_err());
        }

        #[test]
        fn test_resolve_container_root() {
            let runtime = ContainerdRuntime::new();
            let root = runtime.resolve_container_root(12345);
            assert_eq!(root, PathBuf::from("/proc/12345/root"));
        }

        #[test]
        fn test_resolve_container_path() {
            let runtime = ContainerdRuntime::new();
            let path = runtime.resolve_container_path(12345, "/etc/passwd");
            assert_eq!(path, PathBuf::from("/proc/12345/root/etc/passwd"));
        }

        #[test]
        fn test_resolve_container_path_strips_leading_slash() {
            let runtime = ContainerdRuntime::new();
            let path1 = runtime.resolve_container_path(12345, "/etc/passwd");
            let path2 = runtime.resolve_container_path(12345, "etc/passwd");
            assert_eq!(path1, path2);
        }

        #[test]
        fn test_pid_file_path() {
            let runtime = ContainerdRuntime::with_host_root(PathBuf::from("/"));
            let path = runtime.pid_file_path("abc123");
            assert_eq!(
                path,
                PathBuf::from("/run/containerd/io.containerd.runtime.v2.task/k8s.io/abc123/init.pid")
            );
        }

        #[test]
        fn test_pid_file_path_with_host_root() {
            let runtime = ContainerdRuntime::with_host_root(PathBuf::from("/host"));
            let path = runtime.pid_file_path("abc123");
            assert_eq!(
                path,
                PathBuf::from(
                    "/host/run/containerd/io.containerd.runtime.v2.task/k8s.io/abc123/init.pid"
                )
            );
        }
    }

    mod crio_runtime {
        use super::*;

        #[test]
        fn test_strip_prefix_valid() {
            let runtime = CriORuntime::new();
            assert_eq!(runtime.strip_prefix("cri-o://abc123").unwrap(), "abc123");
        }

        #[test]
        fn test_strip_prefix_invalid() {
            let runtime = CriORuntime::new();
            assert!(runtime.strip_prefix("containerd://abc123").is_err());
            assert!(runtime.strip_prefix("abc123").is_err());
        }

        #[test]
        fn test_resolve_container_root() {
            let runtime = CriORuntime::new();
            let root = runtime.resolve_container_root(12345);
            assert_eq!(root, PathBuf::from("/proc/12345/root"));
        }

        #[test]
        fn test_pid_file_path() {
            let runtime = CriORuntime::with_host_root(PathBuf::from("/"));
            let path = runtime.pid_file_path("abc123");
            assert_eq!(path, PathBuf::from("/var/run/crio/abc123/pidfile"));
        }
    }

    mod runtime_for_container {
        use super::*;

        #[test]
        fn test_containerd_id() {
            let runtime = runtime_for_container("containerd://abc123").unwrap();
            assert_eq!(runtime.runtime_type(), RuntimeType::Containerd);
        }

        #[test]
        fn test_crio_id() {
            let runtime = runtime_for_container("cri-o://abc123").unwrap();
            assert_eq!(runtime.runtime_type(), RuntimeType::CriO);
        }

        #[test]
        fn test_unknown_runtime() {
            let result = runtime_for_container("docker://abc123");
            assert!(result.is_err());
            match result {
                Err(e) => assert!(e.to_string().contains("docker://abc123")),
                Ok(_) => panic!("expected error for unknown runtime"),
            }
        }
    }
}
