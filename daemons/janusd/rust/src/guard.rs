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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::sys::fanotify::{
    EventFFlags, Fanotify, FanotifyResponse, InitFlags, MarkFlags, MaskFlags, Response,
};
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::dedupe::DedupeCache;
use crate::policy::{PolicyError, PolicyEvaluator};

/// Errors from the guard module
#[derive(Error, Debug)]
pub enum GuardError {
    #[error("fanotify error: {0}")]
    Fanotify(#[from] nix::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("policy error: {0}")]
    Policy(#[from] PolicyError),

    #[error("permission denied")]
    PermissionDenied,
}

/// Access response type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessResponse {
    Allow,
    Deny,
    Audit,
}

/// Access event from fanotify
#[derive(Debug, Clone)]
pub struct AccessEvent {
    pub event_type: String,
    pub path: String,
    pub filename: String,
    pub is_dir: bool,
    pub pid: i32,
    pub uid: u32,
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
    pub only_dir: bool,
    pub auto_allow_owner: bool,
    pub audit: bool,
    pub enforce: bool,
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
    /// # Errors
    ///
    /// Returns `GuardError::Fanotify` if fanotify_init() fails. Common causes:
    /// - Missing `CAP_SYS_ADMIN` capability
    /// - Kernel doesn't support fanotify
    pub fn new(config: GuardConfig) -> Result<Self, GuardError> {
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
            None, // owner_pid set later via set_owner_pid()
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
        })
    }

    /// Add a mount point to monitor with fanotify.
    ///
    /// Uses `FAN_MARK_MOUNT` to mark the entire mount point, catching all
    /// file access events on the filesystem. The specific events monitored
    /// are determined by the guard's configuration.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to a file on the mount point to monitor
    ///
    /// # Errors
    ///
    /// Returns `GuardError::Fanotify` if fanotify_mark() fails.
    pub fn add_mount(&self, path: &Path) -> Result<(), GuardError> {
        let mask = self.events_to_mask();

        // FAN_MARK_ADD: Add to existing marks
        // FAN_MARK_MOUNT: Apply to entire mount point
        let mark_flags = MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT;

        self.fanotify.mark(mark_flags, mask, None, Some(path))?;

        debug!(path = %path.display(), "Added mount to guard");
        Ok(())
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
    pub async fn guard(
        &self,
        tx: mpsc::Sender<AccessEvent>,
    ) -> Result<(), GuardError> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        info!("Guard started");

        while running.load(Ordering::SeqCst) {
            match self.fanotify.read_events() {
                Ok(events) => {
                    for event in events {
                        let access_event = self.process_event(&event);

                        // Send response for permission events - this MUST happen promptly
                        // to avoid blocking the accessing process
                        if let Some(ref fd) = event.fd() {
                            let response_type = if access_event.response == AccessResponse::Deny {
                                Response::FAN_DENY
                            } else {
                                Response::FAN_ALLOW
                            };

                            let response = FanotifyResponse::new(fd.as_fd(), response_type);

                            if let Err(e) = self.fanotify.write_response(response) {
                                error!(error = %e, "Failed to write fanotify response");
                            }
                        }

                        // Check deduplication BEFORE forwarding to channel
                        // Note: Permission response to kernel already happened above
                        // Deduplication only affects event streaming, not enforcement
                        let should_forward = {
                            let mut dedupe = self.dedupe.lock().await;
                            dedupe.check_and_record(
                                Path::new(&access_event.path),
                                access_event.pid,
                                access_event.response,
                            )
                        };

                        if should_forward {
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

    /// Stop the guard
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn events_to_mask(&self) -> MaskFlags {
        let mut mask = MaskFlags::empty();
        // Use _PERM variants when enforcing (like C daemon) to enable blocking
        let use_perm = self.config.enforce;

        for event in &self.config.events {
            mask |= match event.to_lowercase().as_str() {
                // For access/open, use _PERM variants when enforcing to enable blocking
                "access" => if use_perm { MaskFlags::FAN_ACCESS_PERM } else { MaskFlags::FAN_ACCESS },
                "open" => if use_perm { MaskFlags::FAN_OPEN_PERM } else { MaskFlags::FAN_OPEN },
                // Explicit _perm variants always use permission events
                "open_perm" => MaskFlags::FAN_OPEN_PERM,
                "access_perm" => MaskFlags::FAN_ACCESS_PERM,
                // Non-permission events (notification only)
                "close_write" => MaskFlags::FAN_CLOSE_WRITE,
                "close_nowrite" => MaskFlags::FAN_CLOSE_NOWRITE,
                "modify" => MaskFlags::FAN_MODIFY,
                // Convenience aliases
                "all" => if use_perm {
                    MaskFlags::FAN_ACCESS_PERM | MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_CLOSE | MaskFlags::FAN_MODIFY
                } else {
                    MaskFlags::FAN_ACCESS | MaskFlags::FAN_OPEN | MaskFlags::FAN_CLOSE | MaskFlags::FAN_MODIFY
                },
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

    fn process_event(&self, event: &nix::sys::fanotify::FanotifyEvent) -> AccessEvent {
        // Get raw path from fd
        let raw_path = if let Some(ref fd) = event.fd() {
            let proc_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
            std::fs::read_link(&proc_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Strip /proc/{pid}/root prefix for policy matching (like C daemon)
        // This converts host-relative paths to container-relative paths
        let path = Self::strip_container_prefix(&raw_path);

        let filename = path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();

        // Determine event type
        let event_type = if event.mask().contains(MaskFlags::FAN_ACCESS_PERM)
            || event.mask().contains(MaskFlags::FAN_ACCESS)
        {
            "access"
        } else if event.mask().contains(MaskFlags::FAN_OPEN_PERM)
            || event.mask().contains(MaskFlags::FAN_OPEN)
        {
            "open"
        } else if event.mask().contains(MaskFlags::FAN_CLOSE_WRITE) {
            "close_write"
        } else if event.mask().contains(MaskFlags::FAN_CLOSE_NOWRITE) {
            "close_nowrite"
        } else if event.mask().contains(MaskFlags::FAN_MODIFY) {
            "modify"
        } else {
            "unknown"
        };

        let pid = event.pid();

        // Check if path matches any configured pattern (for log filtering)
        let matched_pattern = self.policy.matches_any_pattern(Path::new(&path));

        // Determine access response using PolicyEvaluator
        // The policy evaluator handles deny/allow patterns, auto_allow_owner,
        // caching, and default response logic
        let response = if self.config.enforce {
            self.policy.evaluate(Path::new(&path), Some(pid))
        } else {
            AccessResponse::Audit
        };

        AccessEvent {
            event_type: event_type.to_string(),
            path,
            filename,
            is_dir: false, // TODO: Check if path is directory
            pid,
            uid: 0, // TODO: Get from /proc/{pid}/status
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

    /// Update the guard's policy with new patterns.
    ///
    /// This allows dynamic policy updates without recreating the guard.
    pub fn update_policy(
        &mut self,
        deny_patterns: Option<Vec<String>>,
        allow_patterns: Option<Vec<String>>,
    ) -> Result<(), GuardError> {
        self.policy
            .update(deny_patterns, allow_patterns, None, None, None)?;
        Ok(())
    }

    /// Set the owner PID for auto_allow_owner feature.
    pub fn set_owner_pid(&mut self, pid: i32) {
        self.policy.set_owner_pid(pid);
    }

    /// Get a reference to the policy evaluator for inspection.
    pub fn policy(&self) -> &PolicyEvaluator {
        &self.policy
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
        assert_eq!(
            Guard::strip_container_prefix("/proc/12345/root"),
            "/"
        );
    }

    #[test]
    fn test_strip_container_prefix_no_prefix() {
        assert_eq!(
            Guard::strip_container_prefix("/etc/shadow"),
            "/etc/shadow"
        );
    }

    #[test]
    fn test_strip_container_prefix_non_proc_path() {
        assert_eq!(
            Guard::strip_container_prefix("/var/lib/containerd/io.containerd.snapshotter/overlayfs/snapshots/123/fs/etc/shadow"),
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
