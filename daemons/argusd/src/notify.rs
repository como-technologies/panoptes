// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # inotify File Integrity Monitoring
//!
//! This module implements file integrity monitoring using the Linux inotify API.
//!
//! ## Kernel Interface
//!
//! inotify provides a mechanism for monitoring filesystem events. Events are
//! read from the inotify file descriptor as `struct inotify_event`:
//!
//! ```c
//! struct inotify_event {
//!     int      wd;       /* Watch descriptor */
//!     uint32_t mask;     /* Mask describing event */
//!     uint32_t cookie;   /* Unique cookie for rename events */
//!     uint32_t len;      /* Size of name field */
//!     char     name[];   /* Optional null-terminated name */
//! };
//! ```
//!
//! ## Event Mask Flags (IN_*)
//!
//! | Flag | Value | Description |
//! |------|-------|-------------|
//! | `IN_ACCESS` | 0x00000001 | File was accessed (read) |
//! | `IN_MODIFY` | 0x00000002 | File was modified (write) |
//! | `IN_ATTRIB` | 0x00000004 | Metadata changed (chmod, chown, etc.) |
//! | `IN_CLOSE_WRITE` | 0x00000008 | File opened for writing was closed |
//! | `IN_CLOSE_NOWRITE` | 0x00000010 | File not opened for writing was closed |
//! | `IN_OPEN` | 0x00000020 | File was opened |
//! | `IN_MOVED_FROM` | 0x00000040 | File moved out of watched directory |
//! | `IN_MOVED_TO` | 0x00000080 | File moved into watched directory |
//! | `IN_CREATE` | 0x00000100 | File/directory created in watched directory |
//! | `IN_DELETE` | 0x00000200 | File/directory deleted from watched directory |
//! | `IN_DELETE_SELF` | 0x00000400 | Watched file/directory was deleted |
//! | `IN_MOVE_SELF` | 0x00000800 | Watched file/directory was moved |
//!
//! ## Cookie-Based Rename Tracking
//!
//! When a file is renamed, the kernel generates two events:
//! 1. `IN_MOVED_FROM` on the source directory
//! 2. `IN_MOVED_TO` on the destination directory
//!
//! Both events share the same `cookie` value, allowing correlation.
//! The cookie is non-zero and unique within the queue lifetime.
//!
//! **Implementation:** Events may arrive out of order in high-load scenarios.
//! We use a 2ms timeout window for pairing (per C implementation).
//!
//! ## Queue Limits
//!
//! - `/proc/sys/fs/inotify/max_user_watches` - Max watches per user (default: 8192)
//! - `/proc/sys/fs/inotify/max_queued_events` - Max events in queue (default: 16384)
//!
//! When the queue overflows, `IN_Q_OVERFLOW` is generated. Recovery requires
//! re-scanning watched directories to detect missed events.
//!
//! ## References
//!
//! - `man 7 inotify` - Overview of inotify API
//! - `man 2 inotify_init` - Initialize inotify instance
//! - `man 2 inotify_add_watch` - Add watch to instance
//! - Linux kernel source: `fs/notify/inotify/`

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use panoptes_common::DaemonMetrics;

use crate::metrics::WatcherMetrics;

/// Default timeout for matching IN_MOVED_FROM with IN_MOVED_TO events.
/// Per C implementation, use 2ms as events may arrive out of order.
pub const MOVE_PAIR_TIMEOUT_MS: u64 = 2;

/// Errors from the notify module.
///
/// These errors occur during inotify initialization, watch management,
/// or event processing.
#[derive(Error, Debug)]
pub enum NotifyError {
    /// inotify system call failed.
    ///
    /// Common causes:
    /// - `EMFILE` - Too many inotify instances
    /// - `ENOMEM` - Insufficient kernel memory
    #[error("inotify error: {0}")]
    Inotify(#[from] nix::Error),

    /// File I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Path does not exist or is not accessible.
    #[allow(dead_code)]
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    /// Maximum watch limit reached.
    ///
    /// The daemon has reached its configured max_watches limit.
    /// This is separate from the kernel's max_user_watches limit.
    #[error("max watches exceeded (limit: {limit}, current: {current})")]
    MaxWatchesExceeded {
        /// Current number of watches.
        current: usize,
        /// Maximum allowed watches.
        limit: usize,
    },

    /// Kernel watch limit reached.
    ///
    /// The kernel's `/proc/sys/fs/inotify/max_user_watches` limit was hit.
    /// Increase the limit or reduce the number of watched paths.
    #[error("kernel watch limit reached (ENOSPC) - increase max_user_watches")]
    KernelWatchLimitReached,

    /// Queue overflow occurred.
    ///
    /// The kernel's event queue overflowed (IN_Q_OVERFLOW). Some events
    /// were lost and a full rescan of watched directories may be needed.
    #[allow(dead_code)]
    #[error("event queue overflow - events may have been lost")]
    QueueOverflow,

    /// Watch descriptor is invalid or stale.
    ///
    /// The watch descriptor was removed (explicitly or due to deletion).
    #[allow(dead_code)]
    #[error("invalid watch descriptor: {0:?}")]
    InvalidWatchDescriptor(WatchDescriptor),

    /// Channel closed unexpectedly.
    #[error("event channel closed")]
    ChannelClosed,
}

/// File event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    Access,
    Attrib,
    CloseWrite,
    CloseNoWrite,
    Create,
    Delete,
    DeleteSelf,
    Modify,
    MoveSelf,
    MovedFrom,
    MovedTo,
    Open,
    Unknown,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Access => "access",
            EventType::Attrib => "attrib",
            EventType::CloseWrite => "closewrite",
            EventType::CloseNoWrite => "closenowrite",
            EventType::Create => "create",
            EventType::Delete => "delete",
            EventType::DeleteSelf => "deleteself",
            EventType::Modify => "modify",
            EventType::MoveSelf => "moveself",
            EventType::MovedFrom => "movedfrom",
            EventType::MovedTo => "movedto",
            EventType::Open => "open",
            EventType::Unknown => "unknown",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "access" => EventType::Access,
            "attrib" => EventType::Attrib,
            "closewrite" | "close_write" => EventType::CloseWrite,
            "closenowrite" | "close_nowrite" => EventType::CloseNoWrite,
            "create" => EventType::Create,
            "delete" => EventType::Delete,
            "deleteself" | "delete_self" => EventType::DeleteSelf,
            "modify" => EventType::Modify,
            "moveself" | "move_self" => EventType::MoveSelf,
            "movedfrom" | "moved_from" => EventType::MovedFrom,
            "movedto" | "moved_to" => EventType::MovedTo,
            "open" => EventType::Open,
            _ => EventType::Unknown,
        }
    }
}

/// A file system event detected by inotify.
///
/// # Cookie Field
///
/// The `cookie` field is used to correlate `MovedFrom` and `MovedTo` events.
/// When a file is renamed, the kernel generates both events with the same
/// non-zero cookie value. A cookie of 0 means this event is not part of a
/// rename operation.
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Type of filesystem event.
    pub event_type: EventType,
    /// Full path to the affected file or directory.
    pub path: PathBuf,
    /// Filename within the watched directory (if applicable).
    pub filename: Option<String>,
    /// Whether the event target is a directory.
    pub is_dir: bool,
    /// Cookie for correlating move events (0 if not a move).
    #[allow(dead_code)]
    pub cookie: u32,
    /// For move events: the paired path (from for MovedTo, to for MovedFrom).
    #[allow(dead_code)]
    pub move_peer: Option<PathBuf>,
}

/// A pending move event awaiting its pair.
#[derive(Debug, Clone)]
pub struct PendingMove {
    /// Path the file was moved from.
    path: PathBuf,
    /// Filename that was moved.
    filename: Option<String>,
    /// Whether it's a directory.
    is_dir: bool,
    /// When this event was recorded.
    timestamp: Instant,
}

/// A non-existent path being watched via its nearest existing ancestor.
///
/// When a configured watch path does not exist at registration time, the
/// watcher places an inotify watch on the nearest ancestor directory and
/// records a `ProxyTarget`. When a CREATE/MOVED_TO event matches the
/// `immediate_child`, the proxy is promoted to a direct watch on the
/// now-existent path.
#[derive(Debug, Clone)]
struct ProxyTarget {
    /// The original configured path (e.g., /proc/123/root/etc/crontab).
    configured_path: PathBuf,
    /// The child name to match under the watched ancestor (e.g., "crontab").
    immediate_child: OsString,
    /// The inotify flags to apply when promoted to a direct watch.
    flags: AddWatchFlags,
}

/// Tracks IN_MOVED_FROM events and matches them with IN_MOVED_TO events.
///
/// # Cookie-Based Correlation
///
/// When a file is renamed, the kernel generates two events with the same
/// cookie value. This tracker holds MOVED_FROM events briefly (2ms default)
/// to allow the corresponding MOVED_TO event to arrive.
///
/// # Timeout Handling
///
/// If no matching MOVED_TO arrives within the timeout, the MOVED_FROM is
/// emitted as an unpaired event (effectively a delete from watched dir).
pub struct MovePairTracker {
    /// Pending MOVED_FROM events, keyed by cookie.
    pending: HashMap<u32, PendingMove>,
    /// Timeout for waiting for pair.
    timeout: Duration,
}

impl MovePairTracker {
    /// Create a new tracker with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            timeout,
        }
    }

    /// Record a MOVED_FROM event. Returns None - caller should wait for pair.
    ///
    /// # Arguments
    ///
    /// * `cookie` - The kernel-assigned cookie for this rename operation
    /// * `path` - Full path to the file being moved
    /// * `filename` - Filename within the watched directory
    /// * `is_dir` - Whether the target is a directory
    pub fn record_moved_from(
        &mut self,
        cookie: u32,
        path: PathBuf,
        filename: Option<String>,
        is_dir: bool,
    ) {
        if cookie == 0 {
            // Cookie 0 means not a paired move event
            return;
        }

        self.pending.insert(
            cookie,
            PendingMove {
                path,
                filename,
                is_dir,
                timestamp: Instant::now(),
            },
        );
    }

    /// Try to match a MOVED_TO event with a pending MOVED_FROM.
    ///
    /// # Returns
    ///
    /// - `Some(PendingMove)` if a matching MOVED_FROM was found
    /// - `None` if no match (file moved from outside watched tree)
    pub fn match_moved_to(&mut self, cookie: u32) -> Option<PendingMove> {
        if cookie == 0 {
            return None;
        }
        self.pending.remove(&cookie)
    }

    /// Remove and return all expired pending moves.
    ///
    /// Call this periodically to emit unpaired MOVED_FROM events as deletes.
    pub fn drain_expired(&mut self) -> Vec<(u32, PendingMove)> {
        let now = Instant::now();
        let expired: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, v)| now.duration_since(v.timestamp) > self.timeout)
            .map(|(k, _)| *k)
            .collect();

        expired
            .into_iter()
            .filter_map(|k| self.pending.remove(&k).map(|v| (k, v)))
            .collect()
    }

    /// Number of pending moves awaiting their pair.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Clear all pending moves.
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Convert event names to inotify flags
pub fn events_to_flags(events: &[String]) -> AddWatchFlags {
    let mut flags = AddWatchFlags::empty();

    for event in events {
        flags |= match event.to_lowercase().as_str() {
            "access" => AddWatchFlags::IN_ACCESS,
            "attrib" => AddWatchFlags::IN_ATTRIB,
            "closewrite" => AddWatchFlags::IN_CLOSE_WRITE,
            "closenowrite" => AddWatchFlags::IN_CLOSE_NOWRITE,
            "close" => AddWatchFlags::IN_CLOSE,
            "create" => AddWatchFlags::IN_CREATE,
            "delete" => AddWatchFlags::IN_DELETE,
            "deleteself" => AddWatchFlags::IN_DELETE_SELF,
            "modify" => AddWatchFlags::IN_MODIFY,
            "moveself" => AddWatchFlags::IN_MOVE_SELF,
            "movedfrom" => AddWatchFlags::IN_MOVED_FROM,
            "movedto" => AddWatchFlags::IN_MOVED_TO,
            "move" => AddWatchFlags::IN_MOVE,
            "open" => AddWatchFlags::IN_OPEN,
            "all" => AddWatchFlags::IN_ALL_EVENTS,
            _ => AddWatchFlags::empty(),
        };
    }

    if flags.is_empty() {
        flags = AddWatchFlags::IN_ALL_EVENTS;
    }

    flags
}

/// Watcher configuration
#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub paths: Vec<PathBuf>,
    pub events: Vec<String>,
    /// Patterns to ignore (not yet implemented in filtering)
    #[allow(dead_code)]
    pub ignore_patterns: Vec<String>,
    pub recursive: bool,
    pub max_depth: Option<u32>,
    /// Per-path skip_if_missing flags, parallel to `paths`.
    /// When true for a path, non-existent paths are silently skipped.
    /// When false, falls back to a proxy watch on the nearest ancestor.
    pub skip_if_missing: Vec<bool>,
}

/// Watch state for pause/resume support.
#[derive(Debug, Clone)]
pub enum WatchState {
    /// Watcher is actively monitoring.
    Running,
    /// Watcher is paused (inotify fd closed, config retained).
    Paused,
    /// Watcher has been stopped.
    Stopped,
}

/// inotify-based file watcher with move pair tracking and metrics.
///
/// # Kernel Interface
///
/// Uses `inotify_init1(2)` with `IN_NONBLOCK | IN_CLOEXEC` flags for
/// non-blocking event reads and close-on-exec for child process safety.
///
/// # Move Event Pairing
///
/// Automatically correlates `IN_MOVED_FROM` and `IN_MOVED_TO` events using
/// the kernel-provided cookie. Unpaired moves (file moved outside watched
/// tree or from outside) are handled after a configurable timeout.
///
/// # Overflow Recovery
///
/// When `IN_Q_OVERFLOW` is detected, the watcher can reinitialize the inotify
/// instance while preserving the watch configuration.
pub struct Watcher {
    /// inotify file descriptor.
    inotify: Inotify,
    /// Map of watch descriptors to paths.
    watches: HashMap<WatchDescriptor, PathBuf>,
    /// Reverse map for path -> wd lookups.
    path_to_wd: HashMap<PathBuf, WatchDescriptor>,
    /// Move event pair tracker.
    move_tracker: MovePairTracker,
    /// Running state flag.
    running: Arc<AtomicBool>,
    /// Current state.
    state: WatchState,
    /// Maximum number of watches allowed.
    max_watches: usize,
    /// Metrics collector (optional).
    metrics: Option<Arc<WatcherMetrics>>,
    /// Original configuration for pause/resume.
    config: Option<WatchConfig>,
    /// Proxy watches for paths that don't exist at registration time.
    /// Maps ancestor directory WD → list of non-existent target paths
    /// being watched through that ancestor.
    proxy_watches: HashMap<WatchDescriptor, Vec<ProxyTarget>>,
}

impl Watcher {
    /// Create a new watcher with the specified maximum watch count.
    ///
    /// # Kernel Behavior
    ///
    /// Calls `inotify_init1(2)` with flags:
    /// - `IN_NONBLOCK` - Non-blocking reads (returns EAGAIN if no events)
    /// - `IN_CLOEXEC` - Close fd on exec() for security
    ///
    /// # Errors
    ///
    /// - `EMFILE` - Per-process inotify instance limit reached
    /// - `ENFILE` - System-wide fd limit reached
    /// - `ENOMEM` - Insufficient kernel memory
    pub fn new(max_watches: usize) -> Result<Self, NotifyError> {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;

        Ok(Self {
            inotify,
            watches: HashMap::new(),
            path_to_wd: HashMap::new(),
            move_tracker: MovePairTracker::new(Duration::from_millis(MOVE_PAIR_TIMEOUT_MS)),
            running: Arc::new(AtomicBool::new(false)),
            state: WatchState::Stopped,
            max_watches,
            metrics: None,
            config: None,
            proxy_watches: HashMap::new(),
        })
    }

    /// Create a watcher with metrics collection.
    pub fn with_metrics(
        max_watches: usize,
        metrics: Arc<WatcherMetrics>,
    ) -> Result<Self, NotifyError> {
        let mut watcher = Self::new(max_watches)?;
        watcher.metrics = Some(metrics);
        Ok(watcher)
    }

    /// Add a watch for a path.
    ///
    /// # Kernel Behavior
    ///
    /// Calls `inotify_add_watch(2)` to add the path to the inotify instance.
    /// If a watch already exists for this path, the masks are OR'd together.
    ///
    /// # Errors
    ///
    /// - `EACCES` - Read access to pathname not permitted
    /// - `ENOSPC` - User watch limit reached (max_user_watches)
    /// - `ENOENT` - pathname does not exist
    pub fn add_watch(
        &mut self,
        path: &Path,
        flags: AddWatchFlags,
    ) -> Result<WatchDescriptor, NotifyError> {
        if self.watches.len() >= self.max_watches {
            return Err(NotifyError::MaxWatchesExceeded {
                current: self.watches.len(),
                limit: self.max_watches,
            });
        }

        let wd = match self.inotify.add_watch(path, flags) {
            Ok(wd) => wd,
            Err(nix::Error::ENOSPC) => return Err(NotifyError::KernelWatchLimitReached),
            Err(e) => return Err(NotifyError::Inotify(e)),
        };

        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.watches.insert(wd, canonical.clone());
        self.path_to_wd.insert(canonical, wd);

        if let Some(ref metrics) = self.metrics {
            metrics.record_watch_added();
        }

        debug!(path = %path.display(), wd = ?wd, "Added watch");
        Ok(wd)
    }

    /// Add a watch for a path, falling back to a proxy watch on the nearest
    /// existing ancestor if the path does not exist.
    ///
    /// Returns `Ok(true)` if a direct watch was added, `Ok(false)` if a proxy
    /// watch was registered on an ancestor directory.
    fn add_watch_or_proxy(
        &mut self,
        path: &Path,
        flags: AddWatchFlags,
    ) -> Result<bool, NotifyError> {
        if path.exists() {
            self.add_watch(path, flags)?;
            return Ok(true);
        }

        // Walk up to find the nearest existing ancestor directory.
        let mut ancestor = path.parent();
        while let Some(dir) = ancestor {
            if dir.exists() && dir.is_dir() {
                let relative = path
                    .strip_prefix(dir)
                    .expect("ancestor is a prefix of path");
                let immediate_child = relative
                    .components()
                    .next()
                    .expect("path != ancestor")
                    .as_os_str()
                    .to_os_string();

                // Watch ancestor with CREATE/MOVED_TO so we can detect
                // the target appearing. inotify OR's masks if a watch
                // already exists for this path.
                let proxy_flags = flags | AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO;
                let wd = self.add_watch(dir, proxy_flags)?;

                self.proxy_watches.entry(wd).or_default().push(ProxyTarget {
                    configured_path: path.to_path_buf(),
                    immediate_child,
                    flags,
                });

                info!(
                    target = %path.display(),
                    ancestor = %dir.display(),
                    "Path does not exist, watching ancestor (proxy watch)"
                );
                return Ok(false);
            }
            ancestor = dir.parent();
        }

        warn!(path = %path.display(), "No existing ancestor found, cannot watch");
        Ok(false)
    }

    /// Add a watch for a path, respecting `skip_if_missing`.
    ///
    /// When `skip_if_missing` is true, non-existent paths are silently skipped.
    /// When false, falls back to a proxy watch on the nearest ancestor.
    fn add_watch_for_path(
        &mut self,
        path: &Path,
        flags: AddWatchFlags,
        skip_if_missing: bool,
    ) -> Result<(), NotifyError> {
        if skip_if_missing {
            if path.exists() {
                self.add_watch(path, flags)?;
            } else {
                warn!(path = %path.display(), "Path does not exist, skipping (skipIfMissing)");
            }
        } else {
            self.add_watch_or_proxy(path, flags)?;
        }
        Ok(())
    }

    /// Remove a watch by descriptor.
    #[allow(dead_code)]
    pub fn remove_watch(&mut self, wd: WatchDescriptor) -> Result<(), NotifyError> {
        if let Err(e) = self.inotify.rm_watch(wd) {
            // EINVAL means the watch was already removed (e.g., path deleted)
            if e != nix::Error::EINVAL {
                return Err(NotifyError::Inotify(e));
            }
        }

        if let Some(path) = self.watches.remove(&wd) {
            self.path_to_wd.remove(&path);
            if let Some(ref metrics) = self.metrics {
                metrics.record_watch_removed();
            }
        }

        Ok(())
    }

    /// Remove a watch by path.
    #[allow(dead_code)]
    pub fn remove_watch_by_path(&mut self, path: &Path) -> Result<(), NotifyError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(wd) = self.path_to_wd.remove(&canonical) {
            self.remove_watch(wd)?;
        }
        Ok(())
    }

    /// Get path for a watch descriptor.
    #[allow(dead_code)]
    pub fn get_path(&self, wd: WatchDescriptor) -> Option<&PathBuf> {
        self.watches.get(&wd)
    }

    /// Get watch descriptor for a path.
    #[allow(dead_code)]
    pub fn get_wd(&self, path: &Path) -> Option<WatchDescriptor> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.path_to_wd.get(&canonical).copied()
    }

    /// Get the number of active watches.
    #[allow(dead_code)]
    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }

    /// Get current watcher state.
    #[allow(dead_code)]
    pub fn state(&self) -> &WatchState {
        &self.state
    }

    /// Get list of all watched paths.
    #[allow(dead_code)]
    pub fn watched_paths(&self) -> Vec<PathBuf> {
        self.watches.values().cloned().collect()
    }

    /// Check if a specific path is being watched.
    #[allow(dead_code)]
    pub fn is_watching(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.path_to_wd.contains_key(&canonical)
    }

    /// Validate cache consistency by checking if watched paths still exist.
    ///
    /// Returns paths that no longer exist and should have their watches removed.
    #[allow(dead_code)]
    pub fn validate_watches(&self) -> Vec<(WatchDescriptor, PathBuf)> {
        self.watches
            .iter()
            .filter(|(_, path)| !path.exists())
            .map(|(wd, path)| (*wd, path.clone()))
            .collect()
    }

    /// Remove stale watches for paths that no longer exist.
    ///
    /// This is called during recovery or periodically for cache consistency.
    #[allow(dead_code)]
    pub fn cleanup_stale_watches(&mut self) -> usize {
        let stale = self.validate_watches();
        let count = stale.len();

        for (wd, path) in stale {
            debug!(path = %path.display(), "Removing stale watch");
            let _ = self.remove_watch(wd);
        }

        count
    }

    /// Add watches for a configuration WITHOUT starting the event loop.
    ///
    /// This is used for SYNCHRONOUS watch registration, which is critical
    /// for the hardening pattern. The watcher-wait init container relies
    /// on watches being registered before it exits.
    ///
    /// Returns the number of watch descriptors added.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let wd_count = watcher.add_watches(&config)?;
    /// // watches are now registered, set watches_ready = true
    /// // then start the event loop separately
    /// watcher.run_event_loop(tx).await?;
    /// ```
    pub fn add_watches(&mut self, config: &WatchConfig) -> Result<usize, NotifyError> {
        let flags = events_to_flags(&config.events);

        // Store config for pause/resume
        self.config = Some(config.clone());

        let mut added = 0;

        // Add watches for all paths (with proxy fallback for non-existent paths
        // unless skip_if_missing is set for that path)
        for (path, &skip) in config.paths.iter().zip(config.skip_if_missing.iter()) {
            self.add_watch_for_path(path, flags, skip)?;
            added += 1;

            // Add recursive watches if enabled and path exists as a directory
            if config.recursive && path.is_dir() {
                let before = self.watches.len();
                self.add_recursive_watches(path, flags, config.max_depth.unwrap_or(0), 0)?;
                added += self.watches.len() - before;
            }
        }

        if let Some(ref metrics) = self.metrics {
            metrics.set_watches_active(self.watches.len() as u64);
        }

        info!(
            watch_count = self.watches.len(),
            added_count = added,
            "inotify watches registered synchronously"
        );

        Ok(added)
    }

    /// Run the event loop for an already-initialized watcher.
    ///
    /// Call this AFTER `add_watches()` to start processing events.
    /// This separation allows synchronous watch registration for the
    /// watcher-wait init container pattern.
    pub async fn run_event_loop(&mut self, tx: mpsc::Sender<FileEvent>) -> Result<(), NotifyError> {
        let config = self.config.clone().ok_or_else(|| {
            NotifyError::Io(std::io::Error::other("no config - call add_watches first"))
        })?;

        self.running.store(true, Ordering::SeqCst);
        self.state = WatchState::Running;
        let running = self.running.clone();

        info!(watch_count = self.watches.len(), "Event loop started");

        // Run the shared event loop
        self.event_loop_inner(&config, &tx, &running).await?;

        self.state = WatchState::Stopped;
        Ok(())
    }

    /// Start watching and send events to channel.
    ///
    /// This method blocks until `stop()` is called or the channel closes.
    /// Note: For hardened deployments, prefer using `add_watches()` + `run_event_loop()`
    /// separately to ensure watches are registered synchronously before returning.
    pub async fn watch(
        &mut self,
        config: WatchConfig,
        tx: mpsc::Sender<FileEvent>,
    ) -> Result<(), NotifyError> {
        let flags = events_to_flags(&config.events);

        // Store config for pause/resume
        self.config = Some(config.clone());

        // Add watches for all paths (with proxy fallback for non-existent paths
        // unless skip_if_missing is set for that path)
        for (path, &skip) in config.paths.iter().zip(config.skip_if_missing.iter()) {
            self.add_watch_for_path(path, flags, skip)?;

            // Add recursive watches if enabled and path exists as a directory
            if config.recursive && path.is_dir() {
                self.add_recursive_watches(path, flags, config.max_depth.unwrap_or(0), 0)?;
            }
        }

        self.running.store(true, Ordering::SeqCst);
        self.state = WatchState::Running;
        let running = self.running.clone();

        if let Some(ref metrics) = self.metrics {
            metrics.set_watches_active(self.watches.len() as u64);
        }

        info!(watch_count = self.watches.len(), "Started watching");

        // Run the shared event loop
        self.event_loop_inner(&config, &tx, &running).await?;

        self.state = WatchState::Stopped;
        Ok(())
    }

    /// Core event loop shared by `watch()` and `run_event_loop()`.
    ///
    /// Reads inotify events and dispatches them to the channel until
    /// the running flag is cleared or the channel closes.
    async fn event_loop_inner(
        &mut self,
        config: &WatchConfig,
        tx: &mpsc::Sender<FileEvent>,
        running: &Arc<AtomicBool>,
    ) -> Result<(), NotifyError> {
        while running.load(Ordering::SeqCst) {
            // Process any expired move pairs
            self.emit_expired_moves(tx).await?;

            // Read and process one batch of events
            if !self.read_and_dispatch_events(config, tx, running).await? {
                break;
            }
        }
        Ok(())
    }

    /// Read one batch of inotify events and dispatch to the channel.
    ///
    /// Returns `Ok(true)` to continue the loop, `Ok(false)` to stop.
    async fn read_and_dispatch_events(
        &mut self,
        config: &WatchConfig,
        tx: &mpsc::Sender<FileEvent>,
        running: &Arc<AtomicBool>,
    ) -> Result<bool, NotifyError> {
        match self.inotify.read_events() {
            Ok(events) => {
                let event_count = events.len() as u64;
                if event_count > 0 {
                    debug!(event_count = event_count, "Read inotify events");
                }

                for event in events {
                    if !self
                        .dispatch_single_event(&event, config, tx, running)
                        .await?
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Err(nix::Error::EAGAIN) => {
                // No events available, sleep briefly
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(true)
            }
            Err(e) => {
                error!(error = %e, "Error reading inotify events");
                if let Some(ref metrics) = self.metrics {
                    metrics.record_error();
                }
                Ok(true)
            }
        }
    }

    /// Dispatch a single inotify event to the channel.
    ///
    /// Handles queue overflow, watch removal, and regular file events.
    /// Returns `Ok(true)` to continue processing, `Ok(false)` if channel closed.
    async fn dispatch_single_event(
        &mut self,
        event: &nix::sys::inotify::InotifyEvent,
        config: &WatchConfig,
        tx: &mpsc::Sender<FileEvent>,
        running: &Arc<AtomicBool>,
    ) -> Result<bool, NotifyError> {
        debug!(
            wd = ?event.wd,
            mask = ?event.mask,
            cookie = event.cookie,
            name = ?event.name,
            "Processing inotify event"
        );

        // Handle queue overflow
        if event.mask.contains(AddWatchFlags::IN_Q_OVERFLOW) {
            warn!("inotify queue overflow detected");
            if let Some(ref metrics) = self.metrics {
                metrics.record_queue_overflow();
            }
            self.handle_overflow(config, tx).await?;
            return Ok(true);
        }

        // Handle watch removal (IN_IGNORED)
        if event.mask.contains(AddWatchFlags::IN_IGNORED) {
            self.handle_watch_removed(event.wd);
            return Ok(true);
        }

        // Check if this event promotes any proxy watches to direct watches
        self.check_proxy_promotion(event);

        // Process regular events
        if let Some(file_events) = self.process_event(event) {
            for file_event in file_events {
                debug!(
                    event_type = %file_event.event_type.as_str(),
                    path = %file_event.path.display(),
                    "Sending file event to channel"
                );
                if let Some(ref metrics) = self.metrics {
                    metrics.record_event_typed(file_event.event_type.as_str());
                }

                if tx.send(file_event).await.is_err() {
                    warn!("Event channel closed");
                    running.store(false, Ordering::SeqCst);
                    return Ok(false);
                }
            }
        } else {
            debug!(wd = ?event.wd, "Event dropped - wd not in watches map");
        }

        Ok(true)
    }

    /// Stop watching.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Pause the watcher, preserving configuration.
    ///
    /// The inotify watches are removed but the config is retained for resume.
    pub fn pause(&mut self) -> Result<WatchConfig, NotifyError> {
        self.stop();

        // Clear all watches
        for wd in self.watches.keys().copied().collect::<Vec<_>>() {
            let _ = self.inotify.rm_watch(wd);
        }
        self.watches.clear();
        self.path_to_wd.clear();
        self.move_tracker.clear();
        self.proxy_watches.clear();

        self.state = WatchState::Paused;

        self.config
            .clone()
            .ok_or_else(|| NotifyError::Io(std::io::Error::other("no config stored")))
    }

    /// Resume a paused watcher.
    pub async fn resume(&mut self, tx: mpsc::Sender<FileEvent>) -> Result<(), NotifyError> {
        let config = self
            .config
            .clone()
            .ok_or_else(|| NotifyError::Io(std::io::Error::other("no config stored")))?;

        // Reinitialize inotify
        self.inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;

        self.watch(config, tx).await
    }

    fn add_recursive_watches(
        &mut self,
        dir: &Path,
        flags: AddWatchFlags,
        max_depth: u32,
        current_depth: u32,
    ) -> Result<(), NotifyError> {
        if max_depth > 0 && current_depth >= max_depth {
            return Ok(());
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(path = %dir.display(), error = %e, "Failed to read directory");
                return Ok(());
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = self.add_watch(&path, flags) {
                    match e {
                        NotifyError::KernelWatchLimitReached => return Err(e),
                        NotifyError::MaxWatchesExceeded { .. } => return Err(e),
                        _ => {
                            warn!(path = %path.display(), error = %e, "Failed to add watch");
                            continue;
                        }
                    }
                }

                self.add_recursive_watches(&path, flags, max_depth, current_depth + 1)?;
            }
        }

        Ok(())
    }

    /// Process an inotify event, handling move pairing.
    ///
    /// Returns 0, 1, or 2 events depending on move pairing status.
    fn process_event(&mut self, event: &nix::sys::inotify::InotifyEvent) -> Option<Vec<FileEvent>> {
        let dir_path = self.watches.get(&event.wd)?.clone();

        let filename = event
            .name
            .as_ref()
            .map(|n| n.to_str().unwrap_or_default().to_string());

        let full_path = if let Some(ref name) = filename {
            dir_path.join(name)
        } else {
            dir_path.clone()
        };

        let event_type = mask_to_event_type(event.mask);
        let is_dir = event.mask.contains(AddWatchFlags::IN_ISDIR);

        // Handle move events with cookie-based pairing
        match event_type {
            EventType::MovedFrom => {
                // Record and wait for matching MovedTo
                self.move_tracker
                    .record_moved_from(event.cookie, full_path, filename, is_dir);
                None
            }
            EventType::MovedTo => {
                // Try to match with pending MovedFrom
                if let Some(pending) = self.move_tracker.match_moved_to(event.cookie) {
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_move_pair_matched();
                    }

                    // Clone pending.path before moving it
                    let pending_path = pending.path.clone();

                    // Emit both events with peer paths
                    Some(vec![
                        FileEvent {
                            event_type: EventType::MovedFrom,
                            path: pending.path,
                            filename: pending.filename,
                            is_dir: pending.is_dir,
                            cookie: event.cookie,
                            move_peer: Some(full_path.clone()),
                        },
                        FileEvent {
                            event_type: EventType::MovedTo,
                            path: full_path,
                            filename,
                            is_dir,
                            cookie: event.cookie,
                            move_peer: Some(pending_path),
                        },
                    ])
                } else {
                    // No matching MovedFrom (file moved from outside watched tree)
                    Some(vec![FileEvent {
                        event_type,
                        path: full_path,
                        filename,
                        is_dir,
                        cookie: event.cookie,
                        move_peer: None,
                    }])
                }
            }
            _ => Some(vec![FileEvent {
                event_type,
                path: full_path,
                filename,
                is_dir,
                cookie: event.cookie,
                move_peer: None,
            }]),
        }
    }

    /// Emit expired move events that didn't get a matching pair.
    async fn emit_expired_moves(
        &mut self,
        tx: &mpsc::Sender<FileEvent>,
    ) -> Result<(), NotifyError> {
        let expired = self.move_tracker.drain_expired();

        for (cookie, pending) in expired {
            if let Some(ref metrics) = self.metrics {
                metrics.record_move_pair_timeout();
            }

            // Emit as unpaired MovedFrom (effectively a delete from watched tree)
            let event = FileEvent {
                event_type: EventType::MovedFrom,
                path: pending.path,
                filename: pending.filename,
                is_dir: pending.is_dir,
                cookie,
                move_peer: None,
            };

            if tx.send(event).await.is_err() {
                return Err(NotifyError::ChannelClosed);
            }
        }

        Ok(())
    }

    /// Handle watch being removed (IN_IGNORED).
    fn handle_watch_removed(&mut self, wd: WatchDescriptor) {
        if let Some(path) = self.watches.remove(&wd) {
            self.path_to_wd.remove(&path);
            if let Some(ref metrics) = self.metrics {
                metrics.record_watch_removed();
            }
            debug!(path = %path.display(), "Watch removed (IN_IGNORED)");
        }
    }

    /// Check if a CREATE/MOVED_TO event promotes a proxy watch to a direct watch.
    ///
    /// When a proxy target's `immediate_child` appears in a watched ancestor
    /// directory, this method either:
    /// - Promotes the proxy to a direct watch if the configured path now exists
    /// - Advances the proxy to the newly created intermediate directory for
    ///   multi-level paths (e.g., `/root/.ssh/authorized_keys`)
    fn check_proxy_promotion(&mut self, event: &nix::sys::inotify::InotifyEvent) {
        if !event.mask.contains(AddWatchFlags::IN_CREATE)
            && !event.mask.contains(AddWatchFlags::IN_MOVED_TO)
        {
            return;
        }

        let event_name = match event.name.as_ref() {
            Some(name) => name,
            None => return,
        };

        let proxies = match self.proxy_watches.get_mut(&event.wd) {
            Some(p) => p,
            None => return,
        };

        // Collect promotions to process after releasing the borrow on proxy_watches.
        let mut promotions: Vec<ProxyTarget> = Vec::new();
        let mut i = 0;
        while i < proxies.len() {
            if proxies[i].immediate_child == *event_name {
                promotions.push(proxies.remove(i));
            } else {
                i += 1;
            }
        }

        if promotions.is_empty() {
            return;
        }

        // Clean up empty proxy entry for this wd
        if let Some(remaining) = self.proxy_watches.get(&event.wd) {
            if remaining.is_empty() {
                self.proxy_watches.remove(&event.wd);
            }
        }

        // Get the ancestor directory path for building child paths
        let ancestor_path = self.watches.get(&event.wd).cloned();

        for proxy in promotions {
            let target = &proxy.configured_path;

            if target.exists() {
                // Target now exists — promote to direct watch
                match self.add_watch(target, proxy.flags) {
                    Ok(_) => {
                        info!(path = %target.display(), "Proxy promoted to direct watch");

                        // If the target is a directory and the original config
                        // was recursive, add recursive watches too. We check
                        // against the stored config.
                        if target.is_dir() {
                            if let Some(ref config) = self.config {
                                if config.recursive {
                                    let _ = self.add_recursive_watches(
                                        target,
                                        proxy.flags,
                                        config.max_depth.unwrap_or(0),
                                        0,
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(path = %target.display(), error = %e, "Failed to promote proxy");
                    }
                }
            } else if let Some(ref anc) = ancestor_path {
                // Multi-level case: the immediate child was created but the
                // full target path still doesn't exist. Advance the proxy
                // to the newly created intermediate directory.
                let child_path = anc.join(event_name.to_str().unwrap_or_default());
                if child_path.is_dir() {
                    if let Ok(relative) = target.strip_prefix(&child_path) {
                        if let Some(next) = relative.components().next() {
                            let next_child = next.as_os_str().to_os_string();
                            let proxy_flags =
                                proxy.flags | AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO;
                            match self.add_watch(&child_path, proxy_flags) {
                                Ok(new_wd) => {
                                    self.proxy_watches.entry(new_wd).or_default().push(
                                        ProxyTarget {
                                            configured_path: proxy.configured_path,
                                            immediate_child: next_child,
                                            flags: proxy.flags,
                                        },
                                    );
                                    info!(
                                        path = %child_path.display(),
                                        "Advanced proxy to closer ancestor"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        path = %child_path.display(),
                                        error = %e,
                                        "Failed to advance proxy"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Handle queue overflow by reinitializing.
    ///
    /// This re-adds all watches from the stored config after the overflow.
    async fn handle_overflow(
        &mut self,
        config: &WatchConfig,
        _tx: &mpsc::Sender<FileEvent>,
    ) -> Result<(), NotifyError> {
        warn!("Reinitializing after queue overflow");

        // Reinitialize inotify
        self.inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;
        self.watches.clear();
        self.path_to_wd.clear();
        self.move_tracker.clear();
        self.proxy_watches.clear();

        // Re-add all watches (with proxy fallback for non-existent paths
        // unless skip_if_missing is set for that path)
        let flags = events_to_flags(&config.events);
        for (path, &skip) in config.paths.iter().zip(config.skip_if_missing.iter()) {
            if let Err(e) = self.add_watch_for_path(path, flags, skip) {
                warn!(path = %path.display(), error = %e, "Failed to re-add watch after overflow");
            }

            if config.recursive && path.is_dir() {
                let _ = self.add_recursive_watches(path, flags, config.max_depth.unwrap_or(0), 0);
            }
        }

        if let Some(ref metrics) = self.metrics {
            metrics.set_watches_active(self.watches.len() as u64);
        }

        info!(
            watch_count = self.watches.len(),
            "Reinitialized after overflow"
        );
        Ok(())
    }
}

fn mask_to_event_type(mask: AddWatchFlags) -> EventType {
    if mask.contains(AddWatchFlags::IN_ACCESS) {
        EventType::Access
    } else if mask.contains(AddWatchFlags::IN_ATTRIB) {
        EventType::Attrib
    } else if mask.contains(AddWatchFlags::IN_CLOSE_WRITE) {
        EventType::CloseWrite
    } else if mask.contains(AddWatchFlags::IN_CLOSE_NOWRITE) {
        EventType::CloseNoWrite
    } else if mask.contains(AddWatchFlags::IN_CREATE) {
        EventType::Create
    } else if mask.contains(AddWatchFlags::IN_DELETE) {
        EventType::Delete
    } else if mask.contains(AddWatchFlags::IN_DELETE_SELF) {
        EventType::DeleteSelf
    } else if mask.contains(AddWatchFlags::IN_MODIFY) {
        EventType::Modify
    } else if mask.contains(AddWatchFlags::IN_MOVE_SELF) {
        EventType::MoveSelf
    } else if mask.contains(AddWatchFlags::IN_MOVED_FROM) {
        EventType::MovedFrom
    } else if mask.contains(AddWatchFlags::IN_MOVED_TO) {
        EventType::MovedTo
    } else if mask.contains(AddWatchFlags::IN_OPEN) {
        EventType::Open
    } else {
        EventType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== EventType Tests ====================

    mod event_type {
        use super::*;

        #[test]
        fn test_event_type_as_str() {
            assert_eq!(EventType::Create.as_str(), "create");
            assert_eq!(EventType::Modify.as_str(), "modify");
            assert_eq!(EventType::Delete.as_str(), "delete");
            assert_eq!(EventType::MovedFrom.as_str(), "movedfrom");
            assert_eq!(EventType::MovedTo.as_str(), "movedto");
            assert_eq!(EventType::Unknown.as_str(), "unknown");
        }

        #[test]
        fn test_event_type_from_str() {
            assert_eq!(EventType::from_str("create"), EventType::Create);
            assert_eq!(EventType::from_str("CREATE"), EventType::Create);
            assert_eq!(EventType::from_str("modify"), EventType::Modify);
            assert_eq!(EventType::from_str("invalid"), EventType::Unknown);
        }

        #[test]
        fn test_event_type_roundtrip() {
            let types = vec![
                EventType::Access,
                EventType::Attrib,
                EventType::CloseWrite,
                EventType::CloseNoWrite,
                EventType::Create,
                EventType::Delete,
                EventType::DeleteSelf,
                EventType::Modify,
                EventType::MoveSelf,
                EventType::MovedFrom,
                EventType::MovedTo,
                EventType::Open,
            ];

            for event_type in types {
                let s = event_type.as_str();
                assert_eq!(EventType::from_str(s), event_type);
            }
        }
    }

    // ==================== MovePairTracker Tests ====================

    mod move_pair_tracker {
        use super::*;

        #[test]
        fn test_record_moved_from() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            assert_eq!(tracker.pending_count(), 1);
        }

        #[test]
        fn test_cookie_zero_ignored() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            // Cookie 0 should be ignored
            tracker.record_moved_from(
                0,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            assert_eq!(tracker.pending_count(), 0);
        }

        #[test]
        fn test_match_moved_to_success() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            let result = tracker.match_moved_to(42);

            assert!(result.is_some());
            let pending = result.unwrap();
            assert_eq!(pending.path, PathBuf::from("/a/file.txt"));
            assert_eq!(pending.filename, Some("file.txt".to_string()));
            assert!(!pending.is_dir);

            // Should be removed after match
            assert_eq!(tracker.pending_count(), 0);
        }

        #[test]
        fn test_match_moved_to_no_match() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            // Different cookie should not match
            let result = tracker.match_moved_to(99);

            assert!(result.is_none());
            assert_eq!(tracker.pending_count(), 1);
        }

        #[test]
        fn test_match_moved_to_cookie_zero() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            // Cookie 0 should return None
            let result = tracker.match_moved_to(0);

            assert!(result.is_none());
        }

        #[test]
        fn test_drain_expired() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(1));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );
            tracker.record_moved_from(
                43,
                PathBuf::from("/b/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            // Wait for expiration
            std::thread::sleep(Duration::from_millis(5));

            let expired = tracker.drain_expired();

            assert_eq!(expired.len(), 2);
            assert_eq!(tracker.pending_count(), 0);
        }

        #[test]
        fn test_drain_expired_partial() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(50));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/file.txt"),
                Some("file.txt".to_string()),
                false,
            );

            // Not expired yet
            let expired = tracker.drain_expired();
            assert_eq!(expired.len(), 0);
            assert_eq!(tracker.pending_count(), 1);

            // Wait for expiration
            std::thread::sleep(Duration::from_millis(60));

            let expired = tracker.drain_expired();
            assert_eq!(expired.len(), 1);
        }

        #[test]
        fn test_clear() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(42, PathBuf::from("/a/file.txt"), None, false);
            tracker.record_moved_from(43, PathBuf::from("/b/file.txt"), None, false);

            assert_eq!(tracker.pending_count(), 2);

            tracker.clear();

            assert_eq!(tracker.pending_count(), 0);
        }

        #[test]
        fn test_directory_move() {
            let mut tracker = MovePairTracker::new(Duration::from_millis(100));

            tracker.record_moved_from(
                42,
                PathBuf::from("/a/subdir"),
                Some("subdir".to_string()),
                true,
            );

            let result = tracker.match_moved_to(42);

            assert!(result.is_some());
            assert!(result.unwrap().is_dir);
        }
    }

    // ==================== events_to_flags Tests ====================

    mod events_to_flags_tests {
        use super::*;

        #[test]
        fn test_single_event() {
            let flags = events_to_flags(&["create".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_CREATE));
        }

        #[test]
        fn test_multiple_events() {
            let flags = events_to_flags(&[
                "create".to_string(),
                "modify".to_string(),
                "delete".to_string(),
            ]);
            assert!(flags.contains(AddWatchFlags::IN_CREATE));
            assert!(flags.contains(AddWatchFlags::IN_MODIFY));
            assert!(flags.contains(AddWatchFlags::IN_DELETE));
        }

        #[test]
        fn test_all_events() {
            let flags = events_to_flags(&["all".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_ALL_EVENTS));
        }

        #[test]
        fn test_empty_defaults_to_all() {
            let flags = events_to_flags(&[]);
            assert!(flags.contains(AddWatchFlags::IN_ALL_EVENTS));
        }

        #[test]
        fn test_case_insensitive() {
            let flags = events_to_flags(&["CREATE".to_string(), "MODIFY".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_CREATE));
            assert!(flags.contains(AddWatchFlags::IN_MODIFY));
        }

        #[test]
        fn test_close_shorthand() {
            let flags = events_to_flags(&["close".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_CLOSE));
        }

        #[test]
        fn test_move_shorthand() {
            let flags = events_to_flags(&["move".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_MOVE));
        }

        #[test]
        fn test_invalid_event_ignored() {
            let flags = events_to_flags(&["invalid_event".to_string(), "create".to_string()]);
            assert!(flags.contains(AddWatchFlags::IN_CREATE));
        }
    }

    // ==================== mask_to_event_type Tests ====================

    mod mask_conversion {
        use super::*;

        #[test]
        fn test_access() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_ACCESS),
                EventType::Access
            );
        }

        #[test]
        fn test_create() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_CREATE),
                EventType::Create
            );
        }

        #[test]
        fn test_modify() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_MODIFY),
                EventType::Modify
            );
        }

        #[test]
        fn test_delete() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_DELETE),
                EventType::Delete
            );
        }

        #[test]
        fn test_moved_from() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_MOVED_FROM),
                EventType::MovedFrom
            );
        }

        #[test]
        fn test_moved_to() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::IN_MOVED_TO),
                EventType::MovedTo
            );
        }

        #[test]
        fn test_unknown() {
            assert_eq!(
                mask_to_event_type(AddWatchFlags::empty()),
                EventType::Unknown
            );
        }
    }

    // ==================== WatchConfig Tests ====================

    mod watch_config {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = WatchConfig {
                paths: vec![PathBuf::from("/tmp")],
                events: vec!["all".to_string()],
                ignore_patterns: vec![],
                recursive: false,
                max_depth: None,
                skip_if_missing: vec![false],
            };

            assert_eq!(config.paths.len(), 1);
            assert!(!config.recursive);
        }

        #[test]
        fn test_recursive_config() {
            let config = WatchConfig {
                paths: vec![PathBuf::from("/tmp")],
                events: vec!["create".to_string(), "modify".to_string()],
                ignore_patterns: vec!["*.tmp".to_string()],
                recursive: true,
                max_depth: Some(5),
                skip_if_missing: vec![false],
            };

            assert!(config.recursive);
            assert_eq!(config.max_depth, Some(5));
            assert_eq!(config.ignore_patterns.len(), 1);
        }
    }

    // ==================== NotifyError Tests ====================

    mod error_tests {
        use super::*;

        #[test]
        fn test_max_watches_exceeded_display() {
            let err = NotifyError::MaxWatchesExceeded {
                current: 100,
                limit: 100,
            };
            let msg = err.to_string();
            assert!(msg.contains("100"));
            assert!(msg.contains("limit"));
        }

        #[test]
        fn test_kernel_watch_limit_display() {
            let err = NotifyError::KernelWatchLimitReached;
            let msg = err.to_string();
            assert!(msg.contains("max_user_watches"));
        }

        #[test]
        fn test_queue_overflow_display() {
            let err = NotifyError::QueueOverflow;
            let msg = err.to_string();
            assert!(msg.contains("overflow"));
        }

        #[test]
        fn test_path_not_found_display() {
            let err = NotifyError::PathNotFound(PathBuf::from("/nonexistent"));
            let msg = err.to_string();
            assert!(msg.contains("/nonexistent"));
        }
    }

    // ==================== FileEvent Tests ====================

    mod file_event {
        use super::*;

        #[test]
        fn test_file_event_creation() {
            let event = FileEvent {
                event_type: EventType::Create,
                path: PathBuf::from("/tmp/test.txt"),
                filename: Some("test.txt".to_string()),
                is_dir: false,
                cookie: 0,
                move_peer: None,
            };

            assert_eq!(event.event_type, EventType::Create);
            assert!(!event.is_dir);
            assert!(event.move_peer.is_none());
        }

        #[test]
        fn test_file_event_with_move_peer() {
            let event = FileEvent {
                event_type: EventType::MovedTo,
                path: PathBuf::from("/tmp/new.txt"),
                filename: Some("new.txt".to_string()),
                is_dir: false,
                cookie: 42,
                move_peer: Some(PathBuf::from("/tmp/old.txt")),
            };

            assert_eq!(event.cookie, 42);
            assert_eq!(event.move_peer, Some(PathBuf::from("/tmp/old.txt")));
        }

        #[test]
        fn test_directory_event() {
            let event = FileEvent {
                event_type: EventType::Create,
                path: PathBuf::from("/tmp/subdir"),
                filename: Some("subdir".to_string()),
                is_dir: true,
                cookie: 0,
                move_peer: None,
            };

            assert!(event.is_dir);
        }
    }

    // ==================== WatchState Tests ====================

    mod watch_state {
        use super::*;

        #[test]
        fn test_watch_state_clone() {
            let state = WatchState::Running;
            let cloned = state.clone();
            assert!(matches!(cloned, WatchState::Running));
        }

        #[test]
        fn test_watch_state_debug() {
            let state = WatchState::Paused;
            let debug_str = format!("{:?}", state);
            assert!(debug_str.contains("Paused"));
        }
    }

    // ==================== Proxy Watch Tests ====================

    mod proxy_watches {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        #[test]
        fn test_add_watch_or_proxy_existing_path() {
            let dir = TempDir::new().unwrap();
            let mut watcher = Watcher::new(64).unwrap();
            let flags = AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MODIFY;

            // Watching an existing directory should add a direct watch
            let result = watcher.add_watch_or_proxy(dir.path(), flags);
            assert!(result.is_ok());
            assert!(result.unwrap()); // true = direct watch
            assert_eq!(watcher.watches.len(), 1);
            assert!(watcher.proxy_watches.is_empty());
        }

        #[test]
        fn test_add_watch_or_proxy_existing_file() {
            let dir = TempDir::new().unwrap();
            let file = dir.path().join("existing.txt");
            fs::write(&file, "hello").unwrap();

            let mut watcher = Watcher::new(64).unwrap();
            let flags = AddWatchFlags::IN_MODIFY;

            let result = watcher.add_watch_or_proxy(&file, flags);
            assert!(result.is_ok());
            assert!(result.unwrap()); // true = direct watch
            assert_eq!(watcher.watches.len(), 1);
            assert!(watcher.proxy_watches.is_empty());
        }

        #[test]
        fn test_add_watch_or_proxy_nonexistent_file() {
            let dir = TempDir::new().unwrap();
            let nonexistent = dir.path().join("does_not_exist.txt");

            let mut watcher = Watcher::new(64).unwrap();
            let flags = AddWatchFlags::IN_MODIFY;

            let result = watcher.add_watch_or_proxy(&nonexistent, flags);
            assert!(result.is_ok());
            assert!(!result.unwrap()); // false = proxy watch

            // Should have a watch on the parent directory
            assert_eq!(watcher.watches.len(), 1);
            let (wd, watched_path) = watcher.watches.iter().next().unwrap();
            assert_eq!(watched_path, &dir.path().canonicalize().unwrap());

            // Should have a proxy target
            assert_eq!(watcher.proxy_watches.len(), 1);
            let proxies = watcher.proxy_watches.get(wd).unwrap();
            assert_eq!(proxies.len(), 1);
            assert_eq!(proxies[0].immediate_child, "does_not_exist.txt");
            assert_eq!(proxies[0].configured_path, nonexistent);
        }

        #[test]
        fn test_add_watch_or_proxy_nonexistent_nested() {
            let dir = TempDir::new().unwrap();
            // Neither "subdir" nor "file.txt" exist
            let nested = dir.path().join("subdir").join("file.txt");

            let mut watcher = Watcher::new(64).unwrap();
            let flags = AddWatchFlags::IN_MODIFY;

            let result = watcher.add_watch_or_proxy(&nested, flags);
            assert!(result.is_ok());
            assert!(!result.unwrap()); // false = proxy watch

            // Should watch the temp dir (nearest existing ancestor)
            assert_eq!(watcher.watches.len(), 1);

            // The immediate child should be "subdir" (first missing component)
            let (wd, _) = watcher.watches.iter().next().unwrap();
            let proxies = watcher.proxy_watches.get(wd).unwrap();
            assert_eq!(proxies[0].immediate_child, "subdir");
            assert_eq!(proxies[0].configured_path, nested);
        }

        #[test]
        fn test_multiple_proxies_on_same_ancestor() {
            let dir = TempDir::new().unwrap();
            let file_a = dir.path().join("a.txt");
            let file_b = dir.path().join("b.txt");

            let mut watcher = Watcher::new(64).unwrap();
            let flags = AddWatchFlags::IN_MODIFY;

            watcher.add_watch_or_proxy(&file_a, flags).unwrap();
            watcher.add_watch_or_proxy(&file_b, flags).unwrap();

            // Both should proxy through the same ancestor watch
            assert_eq!(watcher.watches.len(), 1);
            let (wd, _) = watcher.watches.iter().next().unwrap();
            let proxies = watcher.proxy_watches.get(wd).unwrap();
            assert_eq!(proxies.len(), 2);
        }

        #[test]
        fn test_proxy_watches_cleared_on_pause() {
            let dir = TempDir::new().unwrap();
            let nonexistent = dir.path().join("ghost.txt");

            let mut watcher = Watcher::new(64).unwrap();

            // Set up a proxy watch
            let config = WatchConfig {
                paths: vec![nonexistent],
                events: vec!["modify".to_string()],
                ignore_patterns: vec![],
                recursive: false,
                max_depth: None,
                skip_if_missing: vec![false],
            };
            watcher.add_watches(&config).unwrap();
            assert!(!watcher.proxy_watches.is_empty());

            // Pause should clear proxy watches
            let _ = watcher.pause();
            assert!(watcher.proxy_watches.is_empty());
            assert!(watcher.watches.is_empty());
        }

        #[test]
        fn test_skip_if_missing_skips_nonexistent() {
            let dir = TempDir::new().unwrap();
            let nonexistent = dir.path().join("ghost.txt");

            let mut watcher = Watcher::new(64).unwrap();

            let config = WatchConfig {
                paths: vec![nonexistent],
                events: vec!["modify".to_string()],
                ignore_patterns: vec![],
                recursive: false,
                max_depth: None,
                skip_if_missing: vec![true],
            };
            watcher.add_watches(&config).unwrap();

            // With skip_if_missing, no watches or proxies should be created
            assert!(watcher.watches.is_empty());
            assert!(watcher.proxy_watches.is_empty());
        }

        #[test]
        fn test_skip_if_missing_watches_existing() {
            let dir = TempDir::new().unwrap();
            let file = dir.path().join("exists.txt");
            fs::write(&file, "data").unwrap();

            let mut watcher = Watcher::new(64).unwrap();

            let config = WatchConfig {
                paths: vec![file],
                events: vec!["modify".to_string()],
                ignore_patterns: vec![],
                recursive: false,
                max_depth: None,
                skip_if_missing: vec![true],
            };
            watcher.add_watches(&config).unwrap();

            // Existing path should still get a direct watch
            assert_eq!(watcher.watches.len(), 1);
            assert!(watcher.proxy_watches.is_empty());
        }
    }
}
