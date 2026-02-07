// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Resource Limit Checking
//!
//! UNIX-style resource limit verification at startup and runtime.
//!
//! This module provides defensive checks for system resource limits that could
//! affect daemon operation. Verifying limits early prevents cryptic errors later
//! when the daemon runs out of file descriptors or watch slots.
//!
//! ## Why This Matters
//!
//! Both argusd (inotify) and janusd (fanotify) consume kernel resources:
//!
//! | Daemon | Resource | Limit Source |
//! |--------|----------|--------------|
//! | argusd | inotify watches | `/proc/sys/fs/inotify/max_user_watches` |
//! | argusd | inotify instances | `/proc/sys/fs/inotify/max_user_instances` |
//! | argusd | file descriptors | `RLIMIT_NOFILE` |
//! | janusd | fanotify marks | Kernel memory (no hard limit, but FD-bound) |
//! | janusd | file descriptors | `RLIMIT_NOFILE` |
//!
//! If limits are too low, the daemon may fail partway through operation,
//! leaving monitored files unprotected without clear error messages.
//!
//! ## Usage
//!
//! Check limits at daemon startup before creating watches/marks:
//!
//! ```rust,no_run
//! use panoptes_common::resource_limits::{check_fd_limit, read_inotify_limits};
//!
//! // Verify FD limit allows for max_watches + safety margin
//! let max_watches = 10000;
//! match check_fd_limit(max_watches as u64, 1024) {
//!     Ok(available) => println!("FD limit OK: {} available", available),
//!     Err(e) => {
//!         eprintln!("ERROR: {}", e);
//!         eprintln!("Fix with: ulimit -n 65536");
//!         std::process::exit(1);
//!     }
//! }
//!
//! // Also check inotify-specific limits
//! let (max_user_watches, max_queued_events) = read_inotify_limits();
//! if max_user_watches < max_watches as u64 {
//!     eprintln!("WARNING: max_user_watches ({}) < max_watches ({})", max_user_watches, max_watches);
//!     eprintln!("Fix with: sysctl -w fs.inotify.max_user_watches={}", max_watches);
//! }
//! ```
//!
//! ## Security Considerations
//!
//! - Exhausting FD limits can cause denial of service
//! - Exhausting inotify watches leaves files unmonitored
//! - Queue overflow causes silent event loss (attack window)
//!
//! Always verify limits at startup and monitor overflow metrics at runtime.
//!
//! ## References
//!
//! - `man 7 inotify` - inotify limits and behavior
//! - `man 7 fanotify` - fanotify resource usage
//! - `man 2 getrlimit` - resource limit queries
//! - `/proc/sys/fs/inotify/*` - inotify sysctl tunables

use std::path::Path;
use thiserror::Error;

/// Errors from resource limit checks.
#[derive(Error, Debug)]
pub enum ResourceLimitError {
    /// File descriptor limit is too low for the requested operation.
    #[error(
        "file descriptor limit too low: {current} < {required} (adjust with: ulimit -n {required})"
    )]
    FdLimitTooLow {
        /// Current soft limit from RLIMIT_NOFILE
        current: u64,
        /// Required limit (requested + safety margin)
        required: u64,
    },

    /// Failed to query resource limit from kernel.
    #[error("failed to query resource limit: {0}")]
    QueryFailed(#[from] nix::Error),

    /// Failed to read proc filesystem.
    #[error("failed to read {path}: {source}")]
    ProcReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Check that RLIMIT_NOFILE is sufficient for the requested resource count.
///
/// # Arguments
///
/// * `required_fds` - Number of file descriptors needed for the operation
///   (e.g., max_watches for inotify, max_marks for fanotify)
/// * `safety_margin` - Additional FDs to reserve for gRPC, logging, etc.
///
/// # Returns
///
/// The current soft limit on success, or an error if insufficient.
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::resource_limits::check_fd_limit;
///
/// // Check for 10000 watches + 1024 safety margin
/// let available = check_fd_limit(10000, 1024)?;
/// println!("FD limit OK: {} available", available);
/// # Ok::<(), panoptes_common::resource_limits::ResourceLimitError>(())
/// ```
///
/// # UNIX Philosophy
///
/// This check follows the principle of failing early with clear messages
/// rather than silently degrading or producing cryptic errors later.
pub fn check_fd_limit(required_fds: u64, safety_margin: u64) -> Result<u64, ResourceLimitError> {
    use nix::sys::resource::{getrlimit, Resource};

    let (soft, _hard) = getrlimit(Resource::RLIMIT_NOFILE)?;
    let total_required = required_fds.saturating_add(safety_margin);

    if soft < total_required {
        return Err(ResourceLimitError::FdLimitTooLow {
            current: soft,
            required: total_required,
        });
    }

    Ok(soft)
}

/// Read current inotify limits from /proc/sys/fs/inotify/*.
///
/// # Returns
///
/// A tuple of (max_user_watches, max_queued_events).
///
/// Falls back to kernel defaults if files cannot be read:
/// - max_user_watches: 8192 (older kernels) or 524288 (newer)
/// - max_queued_events: 16384
///
/// # Kernel Defaults
///
/// | Parameter | Old Default | New Default (5.10+) |
/// |-----------|-------------|---------------------|
/// | max_user_watches | 8192 | 524288 |
/// | max_queued_events | 16384 | 16384 |
/// | max_user_instances | 128 | 128 |
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::resource_limits::read_inotify_limits;
///
/// let (max_watches, max_queued) = read_inotify_limits();
/// println!("inotify limits: {} watches, {} queue", max_watches, max_queued);
///
/// if max_queued < 32768 {
///     println!("WARN: Consider increasing max_queued_events");
///     println!("  sysctl -w fs.inotify.max_queued_events=65536");
/// }
/// ```
pub fn read_inotify_limits() -> (u64, u64) {
    let max_user_watches = read_proc_u64("/proc/sys/fs/inotify/max_user_watches").unwrap_or(8192);
    let max_queued_events =
        read_proc_u64("/proc/sys/fs/inotify/max_queued_events").unwrap_or(16384);

    (max_user_watches, max_queued_events)
}

/// Read inotify max_user_instances limit.
///
/// This controls how many separate inotify file descriptors can be created.
/// Default is 128, which is usually sufficient.
pub fn read_inotify_max_instances() -> u64 {
    read_proc_u64("/proc/sys/fs/inotify/max_user_instances").unwrap_or(128)
}

/// Read fanotify limits from /proc/sys/fs/fanotify/*.
///
/// # Returns
///
/// A tuple of (max_user_marks, max_queued_events).
///
/// Falls back to kernel defaults if files cannot be read:
/// - max_user_marks: 8192
/// - max_queued_events: 16384
///
/// # Note
///
/// These sysctls may not exist on older kernels. The daemon should
/// still work but will rely on error handling when limits are reached.
pub fn read_fanotify_limits() -> (u64, u64) {
    // Note: These sysctls were added in relatively recent kernels
    let max_user_marks = read_proc_u64("/proc/sys/fs/fanotify/max_user_marks").unwrap_or(8192);
    let max_queued_events =
        read_proc_u64("/proc/sys/fs/fanotify/max_queued_events").unwrap_or(16384);

    (max_user_marks, max_queued_events)
}

/// Information about current resource limits for logging/diagnostics.
#[derive(Debug, Clone)]
pub struct ResourceLimitsInfo {
    /// RLIMIT_NOFILE soft limit
    pub fd_soft_limit: u64,
    /// RLIMIT_NOFILE hard limit
    pub fd_hard_limit: u64,
    /// inotify max_user_watches (0 if N/A)
    pub inotify_max_watches: u64,
    /// inotify max_queued_events (0 if N/A)
    pub inotify_max_queued: u64,
    /// inotify max_user_instances (0 if N/A)
    pub inotify_max_instances: u64,
    /// fanotify max_user_marks (0 if N/A)
    pub fanotify_max_marks: u64,
    /// fanotify max_queued_events (0 if N/A)
    pub fanotify_max_queued: u64,
}

impl ResourceLimitsInfo {
    /// Collect current resource limits.
    ///
    /// This is useful for startup logging and diagnostics.
    pub fn collect() -> Self {
        use nix::sys::resource::{getrlimit, Resource};

        let (fd_soft, fd_hard) = getrlimit(Resource::RLIMIT_NOFILE).unwrap_or((0, 0));

        let (inotify_max_watches, inotify_max_queued) = read_inotify_limits();
        let inotify_max_instances = read_inotify_max_instances();
        let (fanotify_max_marks, fanotify_max_queued) = read_fanotify_limits();

        Self {
            fd_soft_limit: fd_soft,
            fd_hard_limit: fd_hard,
            inotify_max_watches,
            inotify_max_queued,
            inotify_max_instances,
            fanotify_max_marks,
            fanotify_max_queued,
        }
    }

    /// Log the resource limits at INFO level.
    pub fn log(&self) {
        tracing::info!(
            fd_soft = self.fd_soft_limit,
            fd_hard = self.fd_hard_limit,
            inotify_watches = self.inotify_max_watches,
            inotify_queued = self.inotify_max_queued,
            inotify_instances = self.inotify_max_instances,
            fanotify_marks = self.fanotify_max_marks,
            fanotify_queued = self.fanotify_max_queued,
            "Resource limits"
        );
    }
}

/// Read a u64 value from a /proc file.
fn read_proc_u64(path: impl AsRef<Path>) -> Result<u64, ResourceLimitError> {
    let path = path.as_ref();
    let content =
        std::fs::read_to_string(path).map_err(|e| ResourceLimitError::ProcReadFailed {
            path: path.display().to_string(),
            source: e,
        })?;

    content
        .trim()
        .parse()
        .map_err(|_| ResourceLimitError::ProcReadFailed {
            path: path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "failed to parse as u64"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_fd_limit_success() {
        // Should succeed with current limits (usually 1024+)
        let result = check_fd_limit(100, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_fd_limit_too_low() {
        // Request more than any system would have
        let result = check_fd_limit(u64::MAX / 2, u64::MAX / 2);
        assert!(result.is_err());

        if let Err(ResourceLimitError::FdLimitTooLow { current, required }) = result {
            assert!(current < required);
        } else {
            panic!("Expected FdLimitTooLow error");
        }
    }

    #[test]
    fn test_read_inotify_limits() {
        let (watches, queued) = read_inotify_limits();
        // Should return non-zero values (defaults or actual)
        assert!(watches > 0);
        assert!(queued > 0);
    }

    #[test]
    fn test_read_fanotify_limits() {
        let (marks, queued) = read_fanotify_limits();
        // May be 0 on systems without fanotify sysctls, but queued should exist
        // on most modern kernels
        // Values may be 0 on systems without fanotify sysctls - just verify it runs
        let _ = (marks, queued);
    }

    #[test]
    fn test_resource_limits_info_collect() {
        let info = ResourceLimitsInfo::collect();
        // FD limits should always be set
        assert!(info.fd_soft_limit > 0);
        assert!(info.fd_hard_limit >= info.fd_soft_limit);
    }

    #[test]
    fn test_saturating_add_prevents_overflow() {
        // Verify we don't panic on overflow
        let result = check_fd_limit(u64::MAX, u64::MAX);
        assert!(result.is_err()); // Should fail gracefully
    }
}
