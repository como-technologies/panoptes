// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// fanotify wrapper using nix crate for direct kernel syscalls

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::fcntl::OFlag;
use nix::libc;
use nix::sys::fanotify::{
    EventFFlags, Fanotify, FanotifyResponse, InitFlags, MarkFlags, MaskFlags, Response,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors from the guard module
#[derive(Error, Debug)]
pub enum GuardError {
    #[error("fanotify error: {0}")]
    Fanotify(#[from] nix::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

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

/// fanotify-based file access guard
pub struct Guard {
    fanotify: Fanotify,
    running: Arc<AtomicBool>,
    config: GuardConfig,
}

impl Guard {
    /// Create a new guard
    pub fn new(config: GuardConfig) -> Result<Self, GuardError> {
        // Initialize fanotify with appropriate flags
        let init_flags = if config.enforce {
            InitFlags::FAN_CLASS_CONTENT | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK
        } else {
            InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK
        };

        let event_flags = EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE;

        let fanotify = Fanotify::init(init_flags, event_flags)?;

        Ok(Self {
            fanotify,
            running: Arc::new(AtomicBool::new(false)),
            config,
        })
    }

    /// Add a mount point to monitor
    pub fn add_mount(&self, path: &Path) -> Result<(), GuardError> {
        let mask = self.events_to_mask();

        let mark_flags = MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT;

        self.fanotify.mark(mark_flags, mask, None, Some(path))?;

        debug!(path = %path.display(), "Added mount to guard");
        Ok(())
    }

    /// Start the guard and send events to channel
    pub async fn guard(
        &self,
        tx: mpsc::Sender<AccessEvent>,
    ) -> Result<(), GuardError> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        info!("Guard started");

        // Event buffer
        let mut buf = vec![0u8; 4096];

        while running.load(Ordering::SeqCst) {
            match self.fanotify.read_events() {
                Ok(events) => {
                    for event in events {
                        let access_event = self.process_event(&event);

                        // Send response for permission events
                        if let Some(ref fd) = event.fd() {
                            let response = if access_event.response == AccessResponse::Deny {
                                Response::Deny
                            } else {
                                Response::Allow
                            };

                            if let Err(e) = self.fanotify.write_response(FanotifyResponse::new(
                                fd.as_raw_fd(),
                                response,
                            )) {
                                error!(error = %e, "Failed to write fanotify response");
                            }
                        }

                        if tx.send(access_event).await.is_err() {
                            warn!("Event channel closed");
                            running.store(false, Ordering::SeqCst);
                            break;
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

        for event in &self.config.events {
            mask |= match event.to_lowercase().as_str() {
                "access" => MaskFlags::FAN_ACCESS,
                "open" => MaskFlags::FAN_OPEN,
                "open_perm" => MaskFlags::FAN_OPEN_PERM,
                "access_perm" => MaskFlags::FAN_ACCESS_PERM,
                "close_write" => MaskFlags::FAN_CLOSE_WRITE,
                "close_nowrite" => MaskFlags::FAN_CLOSE_NOWRITE,
                "modify" => MaskFlags::FAN_MODIFY,
                "all" => MaskFlags::FAN_ACCESS | MaskFlags::FAN_OPEN | MaskFlags::FAN_CLOSE | MaskFlags::FAN_MODIFY,
                "all_perm" => MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM,
                _ => MaskFlags::empty(),
            };
        }

        if mask.is_empty() {
            mask = MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM;
        }

        mask
    }

    fn process_event(&self, event: &nix::sys::fanotify::FanotifyEvent) -> AccessEvent {
        // Get path from fd
        let path = if let Some(ref fd) = event.fd() {
            let proc_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
            std::fs::read_link(&proc_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

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

        // Determine access response
        let response = if self.config.enforce {
            self.check_access(&path)
        } else {
            AccessResponse::Audit
        };

        AccessEvent {
            event_type: event_type.to_string(),
            path,
            filename,
            is_dir: false, // TODO: Check if path is directory
            pid: event.pid().map(|p| p.as_raw()).unwrap_or(0),
            uid: 0, // TODO: Get from /proc/{pid}/status
            response,
        }
    }

    fn check_access(&self, path: &str) -> AccessResponse {
        // Check deny patterns first
        for pattern in &self.config.deny_patterns {
            if glob::Pattern::new(pattern)
                .map(|p| p.matches(path))
                .unwrap_or(false)
            {
                return AccessResponse::Deny;
            }
        }

        // Check allow patterns
        for pattern in &self.config.allow_patterns {
            if glob::Pattern::new(pattern)
                .map(|p| p.matches(path))
                .unwrap_or(false)
            {
                return AccessResponse::Allow;
            }
        }

        // Default allow if no deny patterns matched
        if self.config.deny_patterns.is_empty() {
            AccessResponse::Allow
        } else {
            AccessResponse::Deny
        }
    }
}
