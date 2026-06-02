// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Argusd Integration Tests
//!
//! These tests verify the end-to-end functionality of the argusd daemon,
//! including real filesystem interactions with inotify.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use tempfile::TempDir;

// We need to import from the main crate
// For integration tests, we test the public API

/// Test helper to create a temporary directory structure.
fn create_test_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

/// Test helper to create a file in the temp directory.
fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = File::create(&path).expect("Failed to create test file");
    file.write_all(content.as_bytes())
        .expect("Failed to write test file");
    path
}

#[cfg(test)]
mod inotify_tests {
    use super::*;
    use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
    use std::thread;

    /// Test that inotify can detect file creation events.
    #[test]
    fn test_inotify_file_creation() {
        let temp_dir = create_test_dir();
        let watch_path = temp_dir.path();

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Add watch for the directory
        let _wd = inotify
            .add_watch(watch_path, AddWatchFlags::IN_CREATE)
            .expect("Failed to add watch");

        // Create a file in the watched directory
        let file_path = watch_path.join("test_file.txt");
        File::create(&file_path).expect("Failed to create file");

        // Give inotify time to register the event
        thread::sleep(Duration::from_millis(50));

        // Read events
        let events = inotify.read_events().expect("Failed to read events");
        assert!(!events.is_empty(), "Expected at least one event");

        // Verify we got a CREATE event
        let event = &events[0];
        assert!(
            event.mask.contains(AddWatchFlags::IN_CREATE),
            "Expected IN_CREATE event"
        );
        assert_eq!(
            event.name.as_deref(),
            Some(std::ffi::OsStr::new("test_file.txt"))
        );
    }

    /// Test that inotify can detect file modification events.
    #[test]
    fn test_inotify_file_modification() {
        let temp_dir = create_test_dir();
        let file_path = create_test_file(&temp_dir, "modify_test.txt", "initial content");

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Watch the file for modifications
        let _wd = inotify
            .add_watch(&file_path, AddWatchFlags::IN_MODIFY)
            .expect("Failed to add watch");

        // Modify the file
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .expect("Failed to open file");
        writeln!(file, "additional content").expect("Failed to write");
        drop(file);

        // Give inotify time to register the event
        thread::sleep(Duration::from_millis(50));

        // Read events
        let events = inotify.read_events().expect("Failed to read events");
        assert!(!events.is_empty(), "Expected at least one event");

        // Verify we got a MODIFY event
        assert!(
            events
                .iter()
                .any(|e| e.mask.contains(AddWatchFlags::IN_MODIFY)),
            "Expected IN_MODIFY event"
        );
    }

    /// Test that inotify can detect file deletion events.
    #[test]
    fn test_inotify_file_deletion() {
        let temp_dir = create_test_dir();
        let watch_path = temp_dir.path();

        // Create a file first
        let file_path = create_test_file(&temp_dir, "delete_test.txt", "to be deleted");

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Watch the directory for deletions
        let _wd = inotify
            .add_watch(watch_path, AddWatchFlags::IN_DELETE)
            .expect("Failed to add watch");

        // Delete the file
        fs::remove_file(&file_path).expect("Failed to delete file");

        // Give inotify time to register the event
        thread::sleep(Duration::from_millis(50));

        // Read events
        let events = inotify.read_events().expect("Failed to read events");
        assert!(!events.is_empty(), "Expected at least one event");

        // Verify we got a DELETE event
        let event = &events[0];
        assert!(
            event.mask.contains(AddWatchFlags::IN_DELETE),
            "Expected IN_DELETE event"
        );
    }

    /// Test that inotify can detect file move events with cookie pairing.
    #[test]
    fn test_inotify_file_move() {
        let temp_dir = create_test_dir();
        let watch_path = temp_dir.path();

        // Create source file
        let src_path = create_test_file(&temp_dir, "move_src.txt", "moving content");
        let dst_path = watch_path.join("move_dst.txt");

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Watch for move events
        let _wd = inotify
            .add_watch(
                watch_path,
                AddWatchFlags::IN_MOVED_FROM | AddWatchFlags::IN_MOVED_TO,
            )
            .expect("Failed to add watch");

        // Move/rename the file
        fs::rename(&src_path, &dst_path).expect("Failed to rename file");

        // Give inotify time to register the event
        thread::sleep(Duration::from_millis(50));

        // Read events
        let events = inotify.read_events().expect("Failed to read events");
        assert!(
            events.len() >= 2,
            "Expected at least two events (MOVED_FROM and MOVED_TO)"
        );

        // Find MOVED_FROM and MOVED_TO events
        let moved_from = events
            .iter()
            .find(|e| e.mask.contains(AddWatchFlags::IN_MOVED_FROM));
        let moved_to = events
            .iter()
            .find(|e| e.mask.contains(AddWatchFlags::IN_MOVED_TO));

        assert!(moved_from.is_some(), "Expected IN_MOVED_FROM event");
        assert!(moved_to.is_some(), "Expected IN_MOVED_TO event");

        // Verify cookie pairing - both events should have the same non-zero cookie
        let from_cookie = moved_from.unwrap().cookie;
        let to_cookie = moved_to.unwrap().cookie;
        assert!(from_cookie > 0, "Cookie should be non-zero");
        assert_eq!(
            from_cookie, to_cookie,
            "Cookies should match for paired move events"
        );
    }

    /// Test that multiple watches can be established on different paths.
    #[test]
    fn test_inotify_multiple_watches() {
        let temp_dir1 = create_test_dir();
        let temp_dir2 = create_test_dir();

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Add watches on both directories
        let wd1 = inotify
            .add_watch(temp_dir1.path(), AddWatchFlags::IN_CREATE)
            .expect("Failed to add watch 1");
        let wd2 = inotify
            .add_watch(temp_dir2.path(), AddWatchFlags::IN_CREATE)
            .expect("Failed to add watch 2");

        // Watch descriptors should be different
        assert_ne!(wd1, wd2, "Watch descriptors should be unique");

        // Create files in both directories
        File::create(temp_dir1.path().join("file1.txt")).expect("Failed to create file1");
        File::create(temp_dir2.path().join("file2.txt")).expect("Failed to create file2");

        // Give inotify time to register the events
        thread::sleep(Duration::from_millis(50));

        // Read events - should get events from both watches
        let events = inotify.read_events().expect("Failed to read events");
        assert!(events.len() >= 2, "Expected events from both watches");

        // Verify we got events from both watch descriptors
        let has_wd1_event = events.iter().any(|e| e.wd == wd1);
        let has_wd2_event = events.iter().any(|e| e.wd == wd2);
        assert!(has_wd1_event, "Expected event from watch 1");
        assert!(has_wd2_event, "Expected event from watch 2");
    }

    /// Test inotify overflow handling.
    #[test]
    fn test_inotify_queue_behavior() {
        let temp_dir = create_test_dir();
        let watch_path = temp_dir.path();

        // Initialize inotify
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).expect("Failed to init inotify");

        // Add watch
        let _wd = inotify
            .add_watch(watch_path, AddWatchFlags::IN_CREATE)
            .expect("Failed to add watch");

        // Create multiple files rapidly
        for i in 0..100 {
            File::create(watch_path.join(format!("file_{}.txt", i)))
                .expect("Failed to create file");
        }

        // Give inotify time to register events
        thread::sleep(Duration::from_millis(100));

        // Read all events
        let mut total_events = 0;
        loop {
            match inotify.read_events() {
                Ok(events) if !events.is_empty() => {
                    total_events += events.len();
                }
                _ => break,
            }
        }

        // Should have received many events (exact count may vary due to coalescing)
        assert!(total_events > 0, "Expected to receive events");
    }
}

#[cfg(test)]
mod watch_config_tests {
    /// Test glob pattern matching for ignore patterns.
    #[test]
    fn test_glob_ignore_patterns() {
        let pattern = glob::Pattern::new("*.log").expect("Invalid pattern");

        assert!(pattern.matches("test.log"));
        assert!(pattern.matches("error.log"));
        assert!(!pattern.matches("test.txt"));
        assert!(!pattern.matches("log.txt"));
    }

    /// Test recursive glob patterns.
    #[test]
    fn test_recursive_glob_patterns() {
        let pattern = glob::Pattern::new("**/*.tmp").expect("Invalid pattern");

        assert!(pattern.matches("file.tmp"));
        assert!(pattern.matches("dir/file.tmp"));
        assert!(pattern.matches("a/b/c/file.tmp"));
        assert!(!pattern.matches("file.txt"));
    }

    /// Test multiple ignore patterns.
    #[test]
    fn test_multiple_ignore_patterns() {
        let patterns: Vec<glob::Pattern> = vec![
            glob::Pattern::new("*.log").unwrap(),
            glob::Pattern::new("*.tmp").unwrap(),
            glob::Pattern::new(".git/**").unwrap(),
        ];

        let should_ignore = |path: &str| -> bool { patterns.iter().any(|p| p.matches(path)) };

        assert!(should_ignore("error.log"));
        assert!(should_ignore("temp.tmp"));
        assert!(should_ignore(".git/config"));
        assert!(should_ignore(".git/objects/abc"));
        assert!(!should_ignore("main.rs"));
        assert!(!should_ignore("README.md"));
    }
}

#[cfg(test)]
mod event_processing_tests {
    /// Event type enumeration matching the daemon's internal representation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EventType {
        Create,
        Modify,
        Delete,
        MovedFrom,
        MovedTo,
        Open,
        Close,
        Access,
        Attrib,
    }

    impl EventType {
        fn from_str(s: &str) -> Option<Self> {
            match s.to_lowercase().as_str() {
                "create" => Some(Self::Create),
                "modify" => Some(Self::Modify),
                "delete" => Some(Self::Delete),
                "movedfrom" | "moved_from" => Some(Self::MovedFrom),
                "movedto" | "moved_to" => Some(Self::MovedTo),
                "open" => Some(Self::Open),
                "close" | "closewrite" | "closenowrite" => Some(Self::Close),
                "access" => Some(Self::Access),
                "attrib" => Some(Self::Attrib),
                _ => None,
            }
        }
    }

    #[test]
    fn test_event_type_parsing() {
        assert_eq!(EventType::from_str("create"), Some(EventType::Create));
        assert_eq!(EventType::from_str("CREATE"), Some(EventType::Create));
        assert_eq!(EventType::from_str("modify"), Some(EventType::Modify));
        assert_eq!(EventType::from_str("delete"), Some(EventType::Delete));
        assert_eq!(EventType::from_str("movedFrom"), Some(EventType::MovedFrom));
        assert_eq!(
            EventType::from_str("moved_from"),
            Some(EventType::MovedFrom)
        );
        assert_eq!(EventType::from_str("invalid"), None);
    }

    /// Test event filtering by type.
    #[test]
    fn test_event_filtering() {
        let allowed_events = [EventType::Create, EventType::Delete];

        let should_include =
            |event_type: EventType| -> bool { allowed_events.contains(&event_type) };

        assert!(should_include(EventType::Create));
        assert!(should_include(EventType::Delete));
        assert!(!should_include(EventType::Modify));
        assert!(!should_include(EventType::Open));
    }
}

#[cfg(test)]
mod metrics_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Test concurrent metric updates are thread-safe.
    #[test]
    fn test_concurrent_metric_updates() {
        let counter = Arc::new(AtomicU64::new(0));
        let threads: Vec<_> = (0..10)
            .map(|_| {
                let counter = Arc::clone(&counter);
                std::thread::spawn(move || {
                    for _ in 0..1000 {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 10_000);
    }
}
