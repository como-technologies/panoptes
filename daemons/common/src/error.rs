//! # Error Types
//!
//! Common error types for Panoptes daemons.
//!
//! This module provides typed errors using `thiserror` for:
//! - Container runtime operations ([`RuntimeError`])
//! - Process information retrieval ([`ProcError`])
//! - General library errors ([`CommonError`])
//!
//! ## Error Handling Philosophy
//!
//! - All errors are typed and descriptive
//! - No panics in library code
//! - Errors include context for debugging
//! - Security-sensitive errors don't leak information

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for the panoptes-common library.
///
/// This enum wraps all specific error types for convenient propagation.
#[derive(Error, Debug)]
pub enum CommonError {
    /// Container runtime operation failed.
    #[error("runtime error: {0}")]
    Runtime(#[from] RuntimeError),

    /// Process information retrieval failed.
    #[error("proc error: {0}")]
    Proc(#[from] ProcError),

    /// I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Security check failed.
    #[error("security error: {0}")]
    Security(#[from] SecurityError),
}

/// Errors from container runtime operations.
///
/// These errors occur when detecting container runtimes or resolving
/// container PIDs and filesystem paths.
///
/// # Example
///
/// ```rust
/// use panoptes_common::error::RuntimeError;
///
/// fn get_pid(container_id: &str) -> Result<u32, RuntimeError> {
///     if container_id.is_empty() {
///         return Err(RuntimeError::InvalidContainerId {
///             id: container_id.to_string(),
///             reason: "empty container ID".to_string(),
///         });
///     }
///     // ... resolve PID
///     # Ok(1234)
/// }
/// ```
#[derive(Error, Debug)]
pub enum RuntimeError {
    /// No supported container runtime detected.
    ///
    /// Checked locations:
    /// - containerd: `/run/containerd/containerd.sock`
    /// - CRI-O: `/var/run/crio/crio.sock`
    #[error("no supported container runtime detected (checked containerd, CRI-O)")]
    NoRuntimeDetected,

    /// Unknown container runtime prefix in container ID.
    #[error("unknown container runtime in ID: {id}")]
    UnknownRuntime {
        /// The container ID with unrecognized prefix.
        id: String,
    },

    /// Container ID format is invalid.
    #[error("invalid container ID '{id}': {reason}")]
    InvalidContainerId {
        /// The invalid container ID.
        id: String,
        /// Reason the ID is invalid.
        reason: String,
    },

    /// PID file for container not found.
    ///
    /// This typically means:
    /// - Container doesn't exist
    /// - Container has been deleted
    /// - Runtime paths are different than expected
    #[error("PID file not found for container {container_id}: {path}")]
    PidFileNotFound {
        /// The container ID being resolved.
        container_id: String,
        /// Path where PID file was expected.
        path: PathBuf,
    },

    /// Failed to read PID from file.
    #[error("failed to read PID file {path}: {source}")]
    PidFileRead {
        /// Path to the PID file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// PID file contains invalid content.
    #[error("invalid PID in file {path}: expected integer, got '{content}'")]
    InvalidPidContent {
        /// Path to the PID file.
        path: PathBuf,
        /// Content that couldn't be parsed.
        content: String,
    },

    /// Container root filesystem not accessible.
    #[error("container root not accessible: /proc/{pid}/root")]
    ContainerRootNotAccessible {
        /// PID of the container's init process.
        pid: u32,
    },
}

/// Errors from /proc filesystem operations.
///
/// These errors occur when reading process information from `/proc/{pid}/`.
///
/// # Security Note
///
/// Some errors may occur due to permission issues. The daemon requires
/// `CAP_SYS_PTRACE` to access `/proc/{pid}/root` for other processes.
#[derive(Error, Debug)]
pub enum ProcError {
    /// Process does not exist or has exited.
    #[error("process {pid} not found (may have exited)")]
    ProcessNotFound {
        /// PID that was not found.
        pid: u32,
    },

    /// Failed to read /proc/{pid}/stat.
    #[error("failed to read /proc/{pid}/stat: {source}")]
    StatReadError {
        /// PID being queried.
        pid: u32,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse /proc/{pid}/stat content.
    ///
    /// The stat file format is:
    /// ```text
    /// pid (comm) state ppid pgrp session tty_nr tpgid flags ...
    /// ```
    ///
    /// Note: `comm` may contain spaces and parentheses.
    #[error("failed to parse /proc/{pid}/stat: {reason}")]
    StatParseError {
        /// PID being queried.
        pid: u32,
        /// Reason parsing failed.
        reason: String,
    },

    /// Failed to read /proc/{pid}/status.
    #[error("failed to read /proc/{pid}/status: {source}")]
    StatusReadError {
        /// PID being queried.
        pid: u32,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse /proc/{pid}/status content.
    #[error("failed to parse /proc/{pid}/status: {reason}")]
    StatusParseError {
        /// PID being queried.
        pid: u32,
        /// Reason parsing failed.
        reason: String,
    },

    /// Failed to resolve file descriptor path.
    ///
    /// Uses readlink on `/proc/{pid}/fd/{fd}` to get the actual path.
    #[error("failed to resolve fd {fd} for pid {pid}: {source}")]
    FdResolutionError {
        /// PID owning the file descriptor.
        pid: u32,
        /// File descriptor number.
        fd: i32,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Permission denied accessing /proc/{pid}/*.
    ///
    /// Requires `CAP_SYS_PTRACE` for accessing other processes' proc entries.
    #[error("permission denied accessing /proc/{pid}/{file} (requires CAP_SYS_PTRACE)")]
    PermissionDenied {
        /// PID being accessed.
        pid: u32,
        /// File within /proc/{pid}/ that was denied.
        file: String,
    },
}

/// Security-related errors.
///
/// These errors indicate missing capabilities or security policy violations.
#[derive(Error, Debug)]
pub enum SecurityError {
    /// Required Linux capability is missing.
    ///
    /// The daemon requires specific capabilities:
    /// - `CAP_SYS_ADMIN` - fanotify operations
    /// - `CAP_SYS_PTRACE` - /proc access for other processes
    /// - `CAP_DAC_READ_SEARCH` - bypass file read permission checks
    #[error("missing required capability: {capability}")]
    MissingCapability {
        /// Name of the missing capability.
        capability: String,
    },

    /// Failed to read process capabilities.
    #[error("failed to read capabilities: {0}")]
    CapabilityReadError(String),

    /// Path traversal attempt detected.
    #[error("path traversal detected in: {path}")]
    PathTraversal {
        /// Path containing traversal attempt.
        path: PathBuf,
    },

    /// Path is outside allowed root.
    #[error("path {path} is outside allowed root {root}")]
    OutsideAllowedRoot {
        /// Path that was checked.
        path: PathBuf,
        /// Allowed root directory.
        root: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_error_display() {
        let err = RuntimeError::NoRuntimeDetected;
        assert!(err.to_string().contains("no supported container runtime"));

        let err = RuntimeError::UnknownRuntime {
            id: "docker://abc123".to_string(),
        };
        assert!(err.to_string().contains("docker://abc123"));
    }

    #[test]
    fn test_proc_error_display() {
        let err = ProcError::ProcessNotFound { pid: 12345 };
        assert!(err.to_string().contains("12345"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_security_error_display() {
        let err = SecurityError::MissingCapability {
            capability: "CAP_SYS_ADMIN".to_string(),
        };
        assert!(err.to_string().contains("CAP_SYS_ADMIN"));
    }

    #[test]
    fn test_error_conversion() {
        let runtime_err = RuntimeError::NoRuntimeDetected;
        let common_err: CommonError = runtime_err.into();
        assert!(matches!(common_err, CommonError::Runtime(_)));
    }
}
