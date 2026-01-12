// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// inotify wrapper using nix crate for direct kernel syscalls

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors from the notify module
#[derive(Error, Debug)]
pub enum NotifyError {
    #[error("inotify error: {0}")]
    Inotify(#[from] nix::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("max watches exceeded")]
    MaxWatchesExceeded,
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

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "access" => EventType::Access,
            "attrib" => EventType::Attrib,
            "closewrite" => EventType::CloseWrite,
            "closenowrite" => EventType::CloseNoWrite,
            "create" => EventType::Create,
            "delete" => EventType::Delete,
            "deleteself" => EventType::DeleteSelf,
            "modify" => EventType::Modify,
            "moveself" => EventType::MoveSelf,
            "movedfrom" => EventType::MovedFrom,
            "movedto" => EventType::MovedTo,
            "open" => EventType::Open,
            _ => EventType::Unknown,
        }
    }
}

/// A file system event
#[derive(Debug, Clone)]
pub struct FileEvent {
    pub event_type: EventType,
    pub path: PathBuf,
    pub filename: Option<String>,
    pub is_dir: bool,
    pub cookie: u32,
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
    pub ignore_patterns: Vec<String>,
    pub recursive: bool,
    pub max_depth: Option<u32>,
}

/// inotify-based file watcher
pub struct Watcher {
    inotify: Inotify,
    watches: HashMap<WatchDescriptor, PathBuf>,
    running: Arc<AtomicBool>,
    max_watches: usize,
}

impl Watcher {
    /// Create a new watcher
    pub fn new(max_watches: usize) -> Result<Self, NotifyError> {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;

        Ok(Self {
            inotify,
            watches: HashMap::new(),
            running: Arc::new(AtomicBool::new(false)),
            max_watches,
        })
    }

    /// Add a watch for a path
    pub fn add_watch(&mut self, path: &Path, flags: AddWatchFlags) -> Result<WatchDescriptor, NotifyError> {
        if self.watches.len() >= self.max_watches {
            return Err(NotifyError::MaxWatchesExceeded);
        }

        let wd = self.inotify.add_watch(path, flags)?;
        self.watches.insert(wd, path.to_path_buf());

        debug!(path = %path.display(), wd = ?wd, "Added watch");
        Ok(wd)
    }

    /// Remove a watch
    pub fn remove_watch(&mut self, wd: WatchDescriptor) -> Result<(), NotifyError> {
        self.inotify.rm_watch(wd)?;
        self.watches.remove(&wd);
        Ok(())
    }

    /// Get path for a watch descriptor
    pub fn get_path(&self, wd: WatchDescriptor) -> Option<&PathBuf> {
        self.watches.get(&wd)
    }

    /// Get the number of active watches
    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }

    /// Start watching and send events to channel
    pub async fn watch(
        &mut self,
        config: WatchConfig,
        tx: mpsc::Sender<FileEvent>,
    ) -> Result<(), NotifyError> {
        let flags = events_to_flags(&config.events);

        // Add watches for all paths
        for path in &config.paths {
            if !path.exists() {
                warn!(path = %path.display(), "Path does not exist, skipping");
                continue;
            }

            self.add_watch(path, flags)?;

            // Add recursive watches if enabled
            if config.recursive && path.is_dir() {
                self.add_recursive_watches(path, flags, config.max_depth.unwrap_or(0), 0)?;
            }
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        info!(watch_count = self.watches.len(), "Started watching");

        // Event loop
        while running.load(Ordering::SeqCst) {
            match self.inotify.read_events() {
                Ok(events) => {
                    for event in events {
                        if let Some(file_event) = self.process_event(&event) {
                            if tx.send(file_event).await.is_err() {
                                warn!("Event channel closed");
                                running.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
                Err(nix::Error::EAGAIN) => {
                    // No events available, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                Err(e) => {
                    error!(error = %e, "Error reading inotify events");
                }
            }
        }

        Ok(())
    }

    /// Stop watching
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
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

        let entries = std::fs::read_dir(dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = self.add_watch(&path, flags) {
                    warn!(path = %path.display(), error = %e, "Failed to add watch");
                    continue;
                }

                self.add_recursive_watches(&path, flags, max_depth, current_depth + 1)?;
            }
        }

        Ok(())
    }

    fn process_event(&self, event: &nix::sys::inotify::InotifyEvent) -> Option<FileEvent> {
        let dir_path = self.watches.get(&event.wd)?;

        let filename = event.name.as_ref().map(|n| {
            n.to_str().unwrap_or_default().to_string()
        });

        let full_path = if let Some(ref name) = filename {
            dir_path.join(name)
        } else {
            dir_path.clone()
        };

        let event_type = mask_to_event_type(event.mask);
        let is_dir = event.mask.contains(AddWatchFlags::IN_ISDIR);

        Some(FileEvent {
            event_type,
            path: full_path,
            filename,
            is_dir,
            cookie: event.cookie,
        })
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
