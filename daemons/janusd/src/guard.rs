// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # fanotify Permission Events
//!
//! This module implements file access auditing using the Linux fanotify API.
//!
//! ## Kernel Interface
//!
//! fanotify provides filesystem-wide event notification with optional permission
//! control. Unlike inotify, fanotify can intercept events *before* they complete
//! and approve/deny them.
//!
//! Events are read from the fanotify fd as `struct fanotify_event_metadata`:
//!
//! ```c
//! struct fanotify_event_metadata {
//!     __u32 event_len;       /* Total event length */
//!     __u8  vers;            /* API version (FANOTIFY_METADATA_VERSION) */
//!     __u8  reserved;
//!     __u16 metadata_len;    /* sizeof(struct fanotify_event_metadata) */
//!     __aligned_u64 mask;    /* Event mask */
//!     __s32 fd;              /* File descriptor for accessed file */
//!     __s32 pid;             /* PID of process that triggered event */
//! };
//! ```
//!
//! ## Initialization Classes
//!
//! | Class | Description | Permission Events |
//! |-------|-------------|-------------------|
//! | `FAN_CLASS_NOTIF` | Notification only | No |
//! | `FAN_CLASS_CONTENT` | Content modification events | Yes |
//! | `FAN_CLASS_PRE_CONTENT` | Before content modification | Yes |
//!
//! ## Event Mask Flags (FAN_*)
//!
//! **Notification Events:**
//!
//! | Flag | Value | Description |
//! |------|-------|-------------|
//! | `FAN_ACCESS` | 0x00000001 | File was accessed (read) |
//! | `FAN_MODIFY` | 0x00000002 | File was modified (write) |
//! | `FAN_CLOSE_WRITE` | 0x00000008 | File opened for writing was closed |
//! | `FAN_CLOSE_NOWRITE` | 0x00000010 | File opened read-only was closed |
//! | `FAN_OPEN` | 0x00000020 | File was opened |
//! | `FAN_OPEN_EXEC` | 0x00001000 | File was opened for execution |
//!
//! **Permission Events (require response):**
//!
//! | Flag | Value | Description |
//! |------|-------|-------------|
//! | `FAN_OPEN_PERM` | 0x00010000 | Permission to open file |
//! | `FAN_ACCESS_PERM` | 0x00020000 | Permission to read file |
//! | `FAN_OPEN_EXEC_PERM` | 0x00040000 | Permission to execute file |
//!
//! ## Permission Response
//!
//! For permission events, must write `struct fanotify_response` back:
//!
//! ```c
//! struct fanotify_response {
//!     __s32 fd;        /* File descriptor from event */
//!     __u32 response;  /* FAN_ALLOW, FAN_DENY, and/or FAN_AUDIT */
//! };
//! ```
//!
//! | Response | Value | Description |
//! |----------|-------|-------------|
//! | `FAN_ALLOW` | 0x01 | Allow the file operation |
//! | `FAN_DENY` | 0x02 | Deny the file operation (process gets EPERM) |
//! | `FAN_AUDIT` | 0x10 | Generate audit log entry |
//!
//! **Critical:** Permission responses MUST be written promptly. Slow responses
//! block the accessing process. Use `FAN_CLASS_NOTIF` for audit-only mode.
//!
//! ## Capabilities Required
//!
//! - `CAP_SYS_ADMIN` - Required for fanotify_init() and fanotify_mark()
//! - `CAP_DAC_READ_SEARCH` - Optional, for accessing files without permission
//!
//! ## File Descriptor Handling
//!
//! Each event contains an open fd to the accessed file. This fd:
//! - Must be closed by the application after processing
//! - Points to the same file the process is accessing
//! - Can be used with `/proc/self/fd/{fd}` to get the path
//!
//! **Memory Leak Warning:** Failing to close fds will exhaust file descriptors.
//!
//! ## Mark Flags
//!
//! | Flag | Description |
//! |------|-------------|
//! | `FAN_MARK_ADD` | Add events to mark mask |
//! | `FAN_MARK_REMOVE` | Remove events from mark mask |
//! | `FAN_MARK_MOUNT` | Mark entire mount point |
//! | `FAN_MARK_FILESYSTEM` | Mark entire filesystem (Linux 4.20+) |
//! | `FAN_MARK_INODE` | Mark specific inode (default) |
//!
//! ## References
//!
//! - `man 7 fanotify` - Overview of fanotify API
//! - `man 2 fanotify_init` - Initialize fanotify group
//! - `man 2 fanotify_mark` - Add marks to fanotify group
//! - Linux kernel source: `fs/notify/fanotify/`

use std::os::fd::{AsFd, AsRawFd};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::sys::fanotify::{
    EventFFlags, Fanotify, FanotifyResponse, InitFlags, MarkFlags, MaskFlags, Response,
};
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info, warn};

use crate::dedupe::DedupeCache;
use crate::metrics::GuardMetrics;
use crate::policy::{PolicyError, PolicyEvaluator};

// Re-export AccessResponse from audit module for backward compatibility
pub use crate::audit::AccessResponse;

/// Errors from the guard module
#[derive(Error, Debug)]
pub enum GuardError {
    #[error("fanotify error: {0}")]
    Fanotify(#[from] nix::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("policy error: {0}")]
    Policy(#[from] PolicyError),

    /// Maximum fanotify marks exceeded for this guard.
    ///
    /// Each guard has a configurable limit on the number of fanotify marks
    /// it can create. This prevents a single guard from exhausting kernel
    /// resources (max_user_marks sysctl).
    ///
    /// ## Why This Matters
    ///
    /// The kernel has a per-user limit on fanotify marks. If one guard
    /// consumes too many marks, subsequent guards will fail to register
    /// their mounts, leaving containers unprotected.
    ///
    /// ## Mitigation
    ///
    /// - Increase max_marks_per_guard in guard config
    /// - Increase kernel limit: sysctl -w fs.fanotify.max_user_marks=65536
    /// - Reduce the number of containers per guard
    #[error(
        "maximum fanotify marks ({max_marks}) exceeded for this guard (active: {marks_active})"
    )]
    MaxMarksExceeded {
        max_marks: usize,
        marks_active: usize,
    },
}

/// Access event from fanotify
#[derive(Debug, Clone)]
pub struct AccessEvent {
    pub event_type: String,
    pub path: String,
    pub is_dir: bool,
    pub pid: i32,
    pub response: AccessResponse,
    /// True if the path matched any configured allow/deny pattern.
    /// Used to filter logging - only log events for paths we care about.
    pub matched_pattern: bool,
}

/// Guard configuration
#[derive(Debug, Clone)]
pub struct GuardConfig {
    pub allow_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub events: Vec<String>,
    pub auto_allow_owner: bool,
    pub enforce: bool,
    /// Maximum number of fanotify marks this guard can create.
    ///
    /// This is a defensive limit to prevent a single guard from exhausting
    /// the kernel's per-user fanotify mark limit. Each call to `add_mount()`
    /// consumes one mark.
    ///
    /// Default: 100 (sufficient for multi-container pods with room to spare)
    ///
    /// The kernel limit is controlled by:
    ///   /proc/sys/fs/fanotify/max_user_marks (typically 8192 or higher)
    pub max_marks_per_guard: usize,
}

/// fanotify-based file access guard.
///
/// Wraps the Linux fanotify API to provide file access monitoring and
/// permission-based access control on mount points.
pub struct Guard {
    fanotify: Fanotify,
    running: Arc<AtomicBool>,
    config: GuardConfig,
    /// Policy evaluator with caching for access decisions.
    policy: PolicyEvaluator,
    /// Deduplication cache to reduce redundant event processing.
    dedupe: Mutex<DedupeCache>,
    /// Optional metrics collector for tracking guard statistics.
    ///
    /// When provided, the guard will record:
    /// - Event counts by type and response (allow/deny/audit)
    /// - Queue overflow events (FAN_Q_OVERFLOW)
    /// - Permission response write retries/failures
    ///
    /// These metrics are critical for detecting security monitoring gaps.
    metrics: Option<Arc<GuardMetrics>>,
    /// Number of active fanotify marks for this guard.
    ///
    /// Tracks the marks created via `add_mount()`. Used to enforce
    /// `config.max_marks_per_guard` and prevent exhausting kernel resources.
    ///
    /// ## Kernel Resource Tracking
    ///
    /// The kernel tracks marks globally per user via `max_user_marks` sysctl.
    /// This per-guard counter provides:
    /// - Defense against a single guard consuming all marks
    /// - Visibility into mark usage for debugging
    /// - Early failure with clear error message rather than kernel ENOSPC
    marks_active: std::sync::atomic::AtomicUsize,
}

impl Guard {
    /// Create a new fanotify guard.
    ///
    /// # Initialization Classes
    ///
    /// When `config.enforce` is true, uses `FAN_CLASS_CONTENT` which enables
    /// permission events (FAN_OPEN_PERM, FAN_ACCESS_PERM). The kernel will
    /// block the accessing process until we respond with FAN_ALLOW or FAN_DENY.
    ///
    /// When `config.enforce` is false, uses `FAN_CLASS_NOTIF` for notification-only
    /// mode. Events are delivered after the fact and cannot block access.
    ///
    /// # Arguments
    ///
    /// * `config` - Guard configuration with allow/deny patterns and settings
    /// * `metrics` - Optional metrics collector for tracking guard statistics.
    ///   When provided, records event counts, queue overflows, and
    ///   response write failures for security monitoring.
    ///
    /// # Errors
    ///
    /// Returns `GuardError::Fanotify` if fanotify_init() fails. Common causes:
    /// - Missing `CAP_SYS_ADMIN` capability
    /// - Kernel doesn't support fanotify
    pub fn new(
        config: GuardConfig,
        metrics: Option<Arc<GuardMetrics>>,
    ) -> Result<Self, GuardError> {
        // Initialize fanotify with appropriate flags
        // FAN_CLASS_CONTENT: Receive permission events for content access
        // FAN_CLASS_NOTIF: Notification only, no permission events
        // FAN_CLOEXEC: Close fd on exec
        // FAN_NONBLOCK: Non-blocking reads
        let init_flags = if config.enforce {
            InitFlags::FAN_CLASS_CONTENT | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK
        } else {
            InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK
        };

        let event_flags = EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE;

        let fanotify = Fanotify::init(init_flags, event_flags)?;

        // Initialize policy evaluator with config patterns
        // Default response: Always Allow - deny patterns are checked first,
        // so only paths matching deny patterns get denied. Everything else
        // should be allowed by default (like C daemon behavior).
        let default_response = AccessResponse::Allow;

        let policy = PolicyEvaluator::with_config(
            config.deny_patterns.clone(),
            config.allow_patterns.clone(),
            config.auto_allow_owner,
            None, // owner_pid not currently supported
            default_response,
            Some(1024), // LRU cache size
        )?;

        // Initialize deduplication cache with defaults (64 entries, 100ms window)
        let dedupe = Mutex::new(DedupeCache::default());

        Ok(Self {
            fanotify,
            running: Arc::new(AtomicBool::new(false)),
            config,
            policy,
            dedupe,
            metrics,
            marks_active: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Add a mount point to monitor with fanotify.
    ///
    /// Uses `FAN_MARK_MOUNT` to mark the entire mount point, catching all
    /// file access events on the filesystem. The specific events monitored
    /// are determined by the guard's configuration.
    ///
    /// # Mark Limit Enforcement
    ///
    /// This method enforces `config.max_marks_per_guard` to prevent a single
    /// guard from exhausting the kernel's fanotify mark limit. Each successful
    /// call increments the internal mark counter.
    ///
    /// ## Why Per-Guard Limits Matter
    ///
    /// The kernel has a global per-user limit on fanotify marks
    /// (`/proc/sys/fs/fanotify/max_user_marks`). Without per-guard limits:
    /// - A single misconfigured guard could consume all available marks
    /// - Subsequent guards would fail with ENOSPC
    /// - Containers would be left unprotected with unclear error messages
    ///
    /// By failing fast with a clear error, operators can identify and fix
    /// the problematic guard configuration.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to a file on the mount point to monitor
    ///
    /// # Errors
    ///
    /// - `GuardError::MaxMarksExceeded` - Mark limit for this guard reached
    /// - `GuardError::Fanotify` - Kernel fanotify_mark() failed
    pub fn add_mount(&self, path: &Path) -> Result<(), GuardError> {
        // Check per-guard mark limit BEFORE attempting to register.
        // This provides a clear error message instead of cryptic ENOSPC from kernel.
        let current_marks = self.marks_active.load(Ordering::SeqCst);
        if current_marks >= self.config.max_marks_per_guard {
            warn!(
                current = current_marks,
                limit = self.config.max_marks_per_guard,
                path = %path.display(),
                "Per-guard mark limit exceeded - cannot add mount"
            );
            return Err(GuardError::MaxMarksExceeded {
                max_marks: self.config.max_marks_per_guard,
                marks_active: current_marks,
            });
        }

        let mask = self.events_to_mask();

        // FAN_MARK_ADD: Add to existing marks
        // FAN_MARK_MOUNT: Apply to entire mount point
        let mark_flags = MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT;

        self.fanotify.mark(mark_flags, mask, None, Some(path))?;

        // Increment mark counter after successful registration
        let new_count = self.marks_active.fetch_add(1, Ordering::SeqCst) + 1;
        debug!(
            path = %path.display(),
            marks_active = new_count,
            limit = self.config.max_marks_per_guard,
            "Added mount to guard"
        );

        Ok(())
    }

    /// Handle fanotify queue overflow detection.
    ///
    /// The fanotify subsystem has a finite event queue controlled by:
    ///   /proc/sys/fs/fanotify/max_queued_events (default: 16384)
    ///
    /// When events arrive faster than userspace can process them:
    /// 1. Kernel drops oldest events that haven't been read
    /// 2. Sets FAN_Q_OVERFLOW flag on the next readable event
    /// 3. Events continue normally after userspace catches up
    ///
    /// # Security Implication
    ///
    /// Overflow means some file access events were permanently lost.
    /// An attacker could potentially exploit this by generating high
    /// event volume to mask malicious access.
    ///
    /// # Returns
    ///
    /// `true` if this was an overflow event (caller should skip processing),
    /// `false` otherwise
    fn handle_queue_overflow(&self, event: &nix::sys::fanotify::FanotifyEvent) -> bool {
        if event.mask().contains(MaskFlags::FAN_Q_OVERFLOW) {
            warn!(
                "fanotify queue overflow detected - events may have been lost. \
                 Consider increasing /proc/sys/fs/fanotify/max_queued_events"
            );
            if let Some(ref metrics) = self.metrics {
                metrics.record_queue_overflow();
            }
            true
        } else {
            false
        }
    }

    /// Write permission response to fanotify with retry logic.
    ///
    /// For permission events, we MUST respond promptly to avoid blocking
    /// the accessing process. If write_response() fails with EAGAIN, we
    /// retry with brief delays.
    ///
    /// # Critical Behavior
    ///
    /// If all retries fail, access is implicitly allowed to prevent hanging
    /// the monitored process. This is a security trade-off: it's better to
    /// allow access than to cause a DoS.
    ///
    /// # Arguments
    ///
    /// * `event` - The fanotify event with the file descriptor
    /// * `response` - The access decision (allow/deny)
    fn write_permission_response(
        &self,
        event: &nix::sys::fanotify::FanotifyEvent,
        access_response: AccessResponse,
    ) {
        let Some(ref fd) = event.fd() else {
            return;
        };

        let response_type = if access_response == AccessResponse::Deny {
            Response::FAN_DENY
        } else {
            Response::FAN_ALLOW
        };

        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MICROS: u64 = 100;
        let mut write_success = false;

        for attempt in 0..MAX_RETRIES {
            let response = FanotifyResponse::new(fd.as_fd(), response_type);
            match self.fanotify.write_response(response) {
                Ok(()) => {
                    write_success = true;
                    break;
                }
                Err(nix::Error::EAGAIN) => {
                    // Kernel buffer full, brief sleep and retry
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_response_retry();
                    }
                    if attempt < MAX_RETRIES - 1 {
                        std::thread::sleep(std::time::Duration::from_micros(
                            RETRY_DELAY_MICROS * (attempt as u64 + 1),
                        ));
                    }
                }
                Err(e) => {
                    // Non-recoverable error, log and break
                    error!(error = %e, "Failed to write fanotify response");
                    break;
                }
            }
        }

        if !write_success {
            warn!(
                "Permission response write failed after {} retries - \
                 access was implicitly allowed to prevent process hang",
                MAX_RETRIES
            );
            if let Some(ref metrics) = self.metrics {
                metrics.record_response_failure();
            }
        }
    }

    /// Check if event should be forwarded based on deduplication.
    ///
    /// Deduplication only affects event streaming, not enforcement.
    /// The permission response to kernel happens regardless.
    async fn should_forward_event(&self, access_event: &AccessEvent) -> bool {
        let mut dedupe = self.dedupe.lock().await;
        dedupe.check_and_record(
            Path::new(&access_event.path),
            access_event.pid,
            access_event.response,
        )
    }

    /// Start the guard event loop and send events to the channel.
    ///
    /// This is the main event loop that reads fanotify events and processes them.
    /// For permission events, it writes the allow/deny response back to the kernel
    /// before the accessing process can continue.
    ///
    /// # Important
    ///
    /// Permission responses MUST be written promptly. The kernel blocks the
    /// accessing process until a response is received. Slow responses will
    /// cause applications to hang.
    pub async fn guard(&self, tx: mpsc::Sender<AccessEvent>) -> Result<(), GuardError> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        info!("Guard started");

        while running.load(Ordering::SeqCst) {
            match self.fanotify.read_events() {
                Ok(events) => {
                    for event in events {
                        // Check for queue overflow (events may have been lost)
                        if self.handle_queue_overflow(&event) {
                            continue;
                        }

                        // Process the event and determine access decision
                        let access_event = self.process_event(&event);

                        // Write permission response to kernel (must be prompt)
                        self.write_permission_response(&event, access_event.response);

                        // Check deduplication before forwarding to channel
                        if self.should_forward_event(&access_event).await {
                            if tx.send(access_event).await.is_err() {
                                warn!("Event channel closed");
                                running.store(false, Ordering::SeqCst);
                                break;
                            }
                        } else {
                            debug!(
                                path = %access_event.path,
                                pid = access_event.pid,
                                "Event deduplicated"
                            );
                        }
                    }
                }
                Err(nix::Error::EAGAIN) => {
                    // No events available, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                Err(e) => {
                    error!(error = %e, "Error reading fanotify events");
                }
            }
        }

        Ok(())
    }

    fn events_to_mask(&self) -> MaskFlags {
        let mut mask = MaskFlags::empty();
        // Use _PERM variants when enforcing (like C daemon) to enable blocking
        let use_perm = self.config.enforce;

        for event in &self.config.events {
            mask |= match event.to_lowercase().as_str() {
                // For access/open, use _PERM variants when enforcing to enable blocking
                "access" => {
                    if use_perm {
                        MaskFlags::FAN_ACCESS_PERM
                    } else {
                        MaskFlags::FAN_ACCESS
                    }
                }
                "open" => {
                    if use_perm {
                        MaskFlags::FAN_OPEN_PERM
                    } else {
                        MaskFlags::FAN_OPEN
                    }
                }
                // Explicit _perm variants always use permission events
                "open_perm" => MaskFlags::FAN_OPEN_PERM,
                "access_perm" => MaskFlags::FAN_ACCESS_PERM,
                // Non-permission events (notification only)
                "close_write" => MaskFlags::FAN_CLOSE_WRITE,
                "close_nowrite" => MaskFlags::FAN_CLOSE_NOWRITE,
                "modify" => MaskFlags::FAN_MODIFY,
                // Convenience aliases
                "all" => {
                    if use_perm {
                        MaskFlags::FAN_ACCESS_PERM
                            | MaskFlags::FAN_OPEN_PERM
                            | MaskFlags::FAN_CLOSE
                            | MaskFlags::FAN_MODIFY
                    } else {
                        MaskFlags::FAN_ACCESS
                            | MaskFlags::FAN_OPEN
                            | MaskFlags::FAN_CLOSE
                            | MaskFlags::FAN_MODIFY
                    }
                }
                "all_perm" => MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM,
                _ => MaskFlags::empty(),
            };
        }

        if mask.is_empty() {
            // Default to permission events for enforcement
            mask = MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM;
        }

        mask
    }

    /// Resolve the file path from a fanotify event's file descriptor.
    ///
    /// Reads the symlink at `/proc/self/fd/{fd}` to get the actual path
    /// of the accessed file. Returns the raw host path.
    fn resolve_event_path(event: &nix::sys::fanotify::FanotifyEvent) -> String {
        if let Some(ref fd) = event.fd() {
            let proc_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
            std::fs::read_link(&proc_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        }
    }

    /// Determine the event type from fanotify mask flags.
    ///
    /// Maps the kernel's mask flags to human-readable event type strings.
    fn determine_event_type(mask: MaskFlags) -> &'static str {
        if mask.contains(MaskFlags::FAN_ACCESS_PERM) || mask.contains(MaskFlags::FAN_ACCESS) {
            "access"
        } else if mask.contains(MaskFlags::FAN_OPEN_PERM) || mask.contains(MaskFlags::FAN_OPEN) {
            "open"
        } else if mask.contains(MaskFlags::FAN_CLOSE_WRITE) {
            "close_write"
        } else if mask.contains(MaskFlags::FAN_CLOSE_NOWRITE) {
            "close_nowrite"
        } else if mask.contains(MaskFlags::FAN_MODIFY) {
            "modify"
        } else {
            "unknown"
        }
    }

    /// Process a fanotify event into an AccessEvent.
    ///
    /// Resolves the path, determines the event type, evaluates access policy,
    /// and builds the AccessEvent struct for forwarding.
    fn process_event(&self, event: &nix::sys::fanotify::FanotifyEvent) -> AccessEvent {
        // Resolve path from fd and strip container prefix
        let raw_path = Self::resolve_event_path(event);
        let path = Self::strip_container_prefix(&raw_path);

        // Determine event type from mask
        let event_type = Self::determine_event_type(event.mask());

        let pid = event.pid();

        // Check if path matches any configured pattern (for log filtering)
        let matched_pattern = self.policy.matches_any_pattern(Path::new(&path));

        // Determine access response using PolicyEvaluator
        let response = if self.config.enforce {
            self.policy.evaluate(Path::new(&path), Some(pid))
        } else {
            AccessResponse::Audit
        };

        AccessEvent {
            event_type: event_type.to_string(),
            path,
            is_dir: std::fs::metadata(&raw_path)
                .map(|m| m.is_dir())
                .unwrap_or(false),
            pid,
            response,
            matched_pattern,
        }
    }

    /// Strip /proc/{pid}/root prefix from paths for policy matching.
    ///
    /// Fanotify returns absolute host paths, but policy patterns are
    /// container-relative. This matches the C daemon behavior at
    /// janusd_impl.cc:514.
    ///
    /// Examples:
    /// - `/proc/12345/root/etc/shadow` -> `/etc/shadow`
    /// - `/etc/shadow` -> `/etc/shadow` (unchanged)
    fn strip_container_prefix(path: &str) -> String {
        const PROC_PREFIX: &str = "/proc/";
        const ROOT_SUFFIX: &str = "/root";

        if !path.starts_with(PROC_PREFIX) {
            return path.to_string();
        }

        // Find the position after /proc/
        let after_proc = &path[PROC_PREFIX.len()..];

        // Find where the PID ends (next /)
        if let Some(slash_pos) = after_proc.find('/') {
            let pid_part = &after_proc[..slash_pos];
            let rest = &after_proc[slash_pos..];

            // Check if PID is all digits and rest starts with /root
            if pid_part.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(ROOT_SUFFIX) {
                // Return everything after /root
                let stripped = &rest[ROOT_SUFFIX.len()..];
                if stripped.is_empty() {
                    return "/".to_string();
                }
                return stripped.to_string();
            }
        }

        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_container_prefix_with_proc_root() {
        assert_eq!(
            Guard::strip_container_prefix("/proc/12345/root/etc/shadow"),
            "/etc/shadow"
        );
    }

    #[test]
    fn test_strip_container_prefix_root_only() {
        assert_eq!(Guard::strip_container_prefix("/proc/12345/root"), "/");
    }

    #[test]
    fn test_strip_container_prefix_no_prefix() {
        assert_eq!(Guard::strip_container_prefix("/etc/shadow"), "/etc/shadow");
    }

    #[test]
    fn test_strip_container_prefix_non_proc_path() {
        assert_eq!(
            Guard::strip_container_prefix(
                "/var/lib/containerd/io.containerd.snapshotter/overlayfs/snapshots/123/fs/etc/shadow"
            ),
            "/var/lib/containerd/io.containerd.snapshotter/overlayfs/snapshots/123/fs/etc/shadow"
        );
    }

    #[test]
    fn test_strip_container_prefix_not_root() {
        // /proc/123/cwd should not be stripped
        assert_eq!(
            Guard::strip_container_prefix("/proc/123/cwd/etc/shadow"),
            "/proc/123/cwd/etc/shadow"
        );
    }

    #[test]
    fn test_strip_container_prefix_non_numeric_pid() {
        // Non-numeric PID should not match
        assert_eq!(
            Guard::strip_container_prefix("/proc/self/root/etc/shadow"),
            "/proc/self/root/etc/shadow"
        );
    }

    #[test]
    fn test_strip_container_prefix_deeply_nested() {
        assert_eq!(
            Guard::strip_container_prefix("/proc/999/root/var/log/app/debug.log"),
            "/var/log/app/debug.log"
        );
    }
}
