//! # Path Provider Abstraction
//!
//! This module provides traits and implementations for resolving filesystem paths
//! in different deployment environments.
//!
//! ## Design Philosophy
//!
//! Panoptes daemons need to access paths that vary based on deployment:
//! - On host: `/proc`, `/etc`, `/var/run/containerd/...`
//! - In container: `/host/proc`, `/host/etc`, `/host/var/run/containerd/...`
//!
//! Instead of scattering path logic throughout the codebase, all path resolution
//! goes through the `PathProvider` trait. This enables:
//! - Consistent path handling across all daemon code
//! - Easy testing with mock paths
//! - Environment variable overrides for special cases
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::paths::{LinuxPathProvider, PathProvider, RuntimeType};
//!
//! let paths = LinuxPathProvider::new();
//!
//! // Get proc path
//! let proc = paths.proc_path();
//! println!("Proc path: {}", proc.display());
//!
//! // Get runtime socket
//! let socket = paths.runtime_socket(RuntimeType::Containerd);
//! println!("Containerd socket: {}", socket.display());
//!
//! // Resolve container path
//! let container_path = paths.resolve_container_path(1234, std::path::Path::new("/etc/passwd"));
//! println!("Container /etc/passwd: {}", container_path.display());
//! ```

use std::path::{Path, PathBuf};

/// Container runtime types for path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeType {
    /// containerd runtime
    Containerd,
    /// CRI-O runtime
    CriO,
}

impl RuntimeType {
    /// Default socket path for this runtime (relative to host root).
    pub fn default_socket_path(&self) -> &'static str {
        match self {
            Self::Containerd => "run/containerd/containerd.sock",
            Self::CriO => "var/run/crio/crio.sock",
        }
    }
}

/// Trait for providing filesystem paths based on deployment environment.
///
/// All path resolution in Panoptes daemons should go through this trait
/// rather than hardcoding paths.
pub trait PathProvider: Send + Sync {
    /// Root path for host filesystem access.
    ///
    /// - On host: `/`
    /// - In container: `/host` (or configured via `HOST_ROOT_PATH`)
    fn host_root(&self) -> &Path;

    /// Path to proc filesystem.
    ///
    /// - On host: `/proc`
    /// - In container: typically still `/proc` (sees container processes)
    fn proc_path(&self) -> &Path;

    /// Get the socket path for a container runtime.
    ///
    /// Combines host_root with the runtime's default socket path.
    fn runtime_socket(&self, runtime: RuntimeType) -> PathBuf;

    /// Resolve a container-relative path to an absolute host path.
    ///
    /// Given a container PID and a path within that container's filesystem,
    /// returns the host path to access that file.
    ///
    /// Example: For PID 1234 and path "/etc/passwd":
    /// - Returns: `/proc/1234/root/etc/passwd`
    fn resolve_container_path(&self, container_pid: u32, path: &Path) -> PathBuf;

    /// Resolve a path that may be host-relative or absolute.
    ///
    /// If the path starts with `/`, it's treated as host-relative and
    /// prepended with host_root. Otherwise, returned as-is.
    fn resolve_path(&self, path: &Path) -> PathBuf;
}

/// Linux-specific path provider.
///
/// Handles path resolution for Linux deployments, whether running on host
/// or in a container.
#[derive(Debug, Clone)]
pub struct LinuxPathProvider {
    /// Root path for host filesystem (usually "/" or "/host").
    host_root: PathBuf,
    /// Path to proc filesystem.
    proc_path: PathBuf,
}

impl LinuxPathProvider {
    /// Create a new path provider with automatic detection.
    ///
    /// Detects host root from environment and filesystem checks.
    pub fn new() -> Self {
        Self {
            host_root: Self::detect_host_root(),
            proc_path: PathBuf::from("/proc"),
        }
    }

    /// Create a path provider with explicit paths.
    ///
    /// Useful for testing or special deployments.
    pub fn with_paths(host_root: PathBuf, proc_path: PathBuf) -> Self {
        Self {
            host_root,
            proc_path,
        }
    }

    /// Create a path provider with optional overrides.
    ///
    /// Uses automatic detection for any `None` values.
    pub fn with_overrides(host_root: Option<PathBuf>, proc_path: Option<PathBuf>) -> Self {
        Self {
            host_root: host_root.unwrap_or_else(Self::detect_host_root),
            proc_path: proc_path.unwrap_or_else(|| PathBuf::from("/proc")),
        }
    }

    /// Detect the host root path.
    ///
    /// Checks in order:
    /// 1. `HOST_ROOT_PATH` environment variable
    /// 2. `/host` directory (common container mount)
    /// 3. Falls back to `/`
    fn detect_host_root() -> PathBuf {
        // Check environment variable first
        if let Ok(path) = std::env::var("HOST_ROOT_PATH") {
            let p = PathBuf::from(&path);
            if p.exists() && p.is_dir() {
                return p;
            }
        }

        // Check common host mount path
        let host_path = Path::new("/host");
        if host_path.exists() && host_path.is_dir() {
            return host_path.to_path_buf();
        }

        // Default to root
        PathBuf::from("/")
    }
}

impl Default for LinuxPathProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PathProvider for LinuxPathProvider {
    fn host_root(&self) -> &Path {
        &self.host_root
    }

    fn proc_path(&self) -> &Path {
        &self.proc_path
    }

    fn runtime_socket(&self, runtime: RuntimeType) -> PathBuf {
        self.host_root.join(runtime.default_socket_path())
    }

    fn resolve_container_path(&self, container_pid: u32, path: &Path) -> PathBuf {
        // Container filesystem is accessible via /proc/<pid>/root
        let path_str = path.to_string_lossy();
        let relative_path = path_str.trim_start_matches('/');

        self.proc_path
            .join(container_pid.to_string())
            .join("root")
            .join(relative_path)
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();

        if path_str.starts_with('/') {
            // Host-relative path - prepend host root
            let relative = path_str.trim_start_matches('/');
            self.host_root.join(relative)
        } else {
            // Already relative - return as-is
            path.to_path_buf()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_type_socket_paths() {
        assert_eq!(
            RuntimeType::Containerd.default_socket_path(),
            "run/containerd/containerd.sock"
        );
        assert_eq!(
            RuntimeType::CriO.default_socket_path(),
            "var/run/crio/crio.sock"
        );
    }

    #[test]
    fn test_linux_path_provider_creation() {
        let provider = LinuxPathProvider::new();
        // Should not panic
        let _ = provider.host_root();
        let _ = provider.proc_path();
    }

    #[test]
    fn test_runtime_socket_on_host() {
        let provider = LinuxPathProvider::with_paths(PathBuf::from("/"), PathBuf::from("/proc"));

        assert_eq!(
            provider.runtime_socket(RuntimeType::Containerd),
            PathBuf::from("/run/containerd/containerd.sock")
        );
        assert_eq!(
            provider.runtime_socket(RuntimeType::CriO),
            PathBuf::from("/var/run/crio/crio.sock")
        );
    }

    #[test]
    fn test_runtime_socket_in_container() {
        let provider =
            LinuxPathProvider::with_paths(PathBuf::from("/host"), PathBuf::from("/proc"));

        assert_eq!(
            provider.runtime_socket(RuntimeType::Containerd),
            PathBuf::from("/host/run/containerd/containerd.sock")
        );
    }

    #[test]
    fn test_resolve_container_path() {
        let provider = LinuxPathProvider::with_paths(PathBuf::from("/"), PathBuf::from("/proc"));

        assert_eq!(
            provider.resolve_container_path(1234, Path::new("/etc/passwd")),
            PathBuf::from("/proc/1234/root/etc/passwd")
        );

        assert_eq!(
            provider.resolve_container_path(5678, Path::new("/var/log/messages")),
            PathBuf::from("/proc/5678/root/var/log/messages")
        );
    }

    #[test]
    fn test_resolve_path_absolute() {
        let provider =
            LinuxPathProvider::with_paths(PathBuf::from("/host"), PathBuf::from("/proc"));

        assert_eq!(
            provider.resolve_path(Path::new("/etc/passwd")),
            PathBuf::from("/host/etc/passwd")
        );
    }

    #[test]
    fn test_resolve_path_relative() {
        let provider =
            LinuxPathProvider::with_paths(PathBuf::from("/host"), PathBuf::from("/proc"));

        assert_eq!(
            provider.resolve_path(Path::new("relative/path")),
            PathBuf::from("relative/path")
        );
    }

    #[test]
    fn test_with_overrides() {
        let provider = LinuxPathProvider::with_overrides(Some(PathBuf::from("/custom")), None);

        assert_eq!(provider.host_root(), Path::new("/custom"));
        assert_eq!(provider.proc_path(), Path::new("/proc"));
    }
}
