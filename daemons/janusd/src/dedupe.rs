// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Event Deduplication for fanotify
//!
//! This module provides event deduplication for fanotify permission events.
//! A single file operation may generate multiple permission events (e.g., a
//! read() syscall may trigger multiple FAN_ACCESS_PERM events). This module
//! implements a time-based deduplication cache to reduce redundant processing.
//!
//! ## Design
//!
//! - **Time Window**: Events within 100ms are considered duplicates
//! - **Cache Size**: Fixed 64-entry circular buffer per C implementation
//! - **Eviction**: Oldest entries evicted when cache is full
//! - **Key**: (path, pid, response) tuple identifies unique events
//!
//! ## Security Considerations
//!
//! - Deduplication should never affect policy enforcement correctness
//! - Denied events must still be logged even if deduplicated for streaming
//! - The cache size is bounded to prevent memory exhaustion attacks

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::audit::AccessResponse;

/// Default deduplication window (100ms per C implementation).
pub const DEFAULT_DEDUPE_WINDOW: Duration = Duration::from_millis(100);

/// Default maximum cache size (64 entries per C implementation).
pub const DEFAULT_CACHE_SIZE: usize = 64;

/// A single deduplication cache entry.
#[derive(Debug, Clone)]
struct DedupeEntry {
    /// The file path that was accessed.
    path: PathBuf,
    /// The process ID that accessed the file.
    pid: i32,
    /// The access response (allow/deny/audit).
    response: AccessResponse,
    /// When this entry was recorded.
    timestamp: Instant,
}

impl DedupeEntry {
    fn new(path: PathBuf, pid: i32, response: AccessResponse) -> Self {
        Self {
            path,
            pid,
            response,
            timestamp: Instant::now(),
        }
    }

    /// Check if this entry matches the given parameters.
    fn matches(&self, path: &Path, pid: i32, response: AccessResponse) -> bool {
        self.path == path && self.pid == pid && self.response == response
    }

    /// Check if this entry has expired based on the given window.
    fn is_expired(&self, window: Duration) -> bool {
        self.timestamp.elapsed() > window
    }
}

/// Event deduplication cache using a circular buffer.
///
/// This cache tracks recent access events to identify duplicates within
/// a configurable time window. Events with the same path, PID, and response
/// within the window are considered duplicates.
#[derive(Debug)]
pub struct DedupeCache {
    /// Circular buffer of recent events.
    entries: VecDeque<DedupeEntry>,
    /// Maximum number of entries to keep.
    max_size: usize,
    /// Time window for deduplication.
    window: Duration,
}

impl Default for DedupeCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_SIZE, DEFAULT_DEDUPE_WINDOW)
    }
}

impl DedupeCache {
    /// Create a new deduplication cache.
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum number of entries (0 = unlimited, but not recommended)
    /// * `window` - Time window for considering events as duplicates
    pub fn new(max_size: usize, window: Duration) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_size.min(64)),
            max_size,
            window,
        }
    }

    /// Check if an event should be processed or is a duplicate.
    ///
    /// Returns `true` if the event should be processed (not a duplicate),
    /// `false` if it's a duplicate and should be skipped.
    ///
    /// This method also cleans up expired entries.
    pub fn should_process(&mut self, path: &Path, pid: i32, response: AccessResponse) -> bool {
        // First, clean up expired entries
        self.clear_expired();

        // Check if this event matches any recent entry
        for entry in &self.entries {
            if entry.matches(path, pid, response) {
                return false; // Duplicate found
            }
        }

        true // Not a duplicate
    }

    /// Record an event in the cache.
    ///
    /// Should be called after `should_process` returns `true`.
    pub fn record(&mut self, path: PathBuf, pid: i32, response: AccessResponse) {
        // Evict oldest entry if at capacity
        if self.max_size > 0 && self.entries.len() >= self.max_size {
            self.entries.pop_front();
        }

        self.entries
            .push_back(DedupeEntry::new(path, pid, response));
    }

    /// Check and record in a single operation.
    ///
    /// Returns `true` if the event should be processed (not a duplicate).
    /// If true, the event is automatically recorded in the cache.
    pub fn check_and_record(&mut self, path: &Path, pid: i32, response: AccessResponse) -> bool {
        if self.should_process(path, pid, response) {
            self.record(path.to_path_buf(), pid, response);
            true
        } else {
            false
        }
    }

    /// Clear all expired entries from the cache.
    pub fn clear_expired(&mut self) {
        self.entries.retain(|e| !e.is_expired(self.window));
    }

    /// Get the current number of entries in the cache.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the cache.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_dedupe_cache_new() {
        let cache = DedupeCache::new(64, Duration::from_millis(100));
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_dedupe_cache_default() {
        let cache = DedupeCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.max_size, DEFAULT_CACHE_SIZE);
    }

    #[test]
    fn test_dedupe_same_event_within_window() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let path = Path::new("/etc/passwd");

        // First event should be processed
        assert!(cache.should_process(path, 1234, AccessResponse::Allow));
        cache.record(path.to_path_buf(), 1234, AccessResponse::Allow);

        // Same event should be a duplicate
        assert!(!cache.should_process(path, 1234, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_different_path() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));

        // First event
        let path1 = Path::new("/etc/passwd");
        assert!(cache.should_process(path1, 1234, AccessResponse::Allow));
        cache.record(path1.to_path_buf(), 1234, AccessResponse::Allow);

        // Different path should be processed
        let path2 = Path::new("/etc/shadow");
        assert!(cache.should_process(path2, 1234, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_different_pid() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let path = Path::new("/etc/passwd");

        // First event with PID 1234
        assert!(cache.should_process(path, 1234, AccessResponse::Allow));
        cache.record(path.to_path_buf(), 1234, AccessResponse::Allow);

        // Same path but different PID should be processed
        assert!(cache.should_process(path, 5678, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_different_response() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let path = Path::new("/etc/passwd");

        // First event with Allow
        assert!(cache.should_process(path, 1234, AccessResponse::Allow));
        cache.record(path.to_path_buf(), 1234, AccessResponse::Allow);

        // Same path and PID but different response should be processed
        assert!(cache.should_process(path, 1234, AccessResponse::Deny));
    }

    #[test]
    fn test_dedupe_expired_entry() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(5));
        let path = Path::new("/etc/passwd");

        // First event
        assert!(cache.should_process(path, 1234, AccessResponse::Allow));
        cache.record(path.to_path_buf(), 1234, AccessResponse::Allow);

        // Wait for expiration
        thread::sleep(Duration::from_millis(10));

        // Same event should now be processed (expired)
        assert!(cache.should_process(path, 1234, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_cache_size_limit() {
        let mut cache = DedupeCache::new(3, Duration::from_secs(10));

        // Fill the cache
        for i in 0..3 {
            let path = PathBuf::from(format!("/path/{}", i));
            cache.record(path, 1234, AccessResponse::Allow);
        }
        assert_eq!(cache.len(), 3);

        // Adding one more should evict the oldest
        cache.record(PathBuf::from("/path/3"), 1234, AccessResponse::Allow);
        assert_eq!(cache.len(), 3);

        // The first entry should have been evicted
        assert!(cache.should_process(Path::new("/path/0"), 1234, AccessResponse::Allow));

        // The second entry should still be there
        assert!(!cache.should_process(Path::new("/path/1"), 1234, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_check_and_record() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let path = Path::new("/etc/passwd");

        // First call should return true and record
        assert!(cache.check_and_record(path, 1234, AccessResponse::Allow));
        assert_eq!(cache.len(), 1);

        // Second call should return false (duplicate)
        assert!(!cache.check_and_record(path, 1234, AccessResponse::Allow));
        assert_eq!(cache.len(), 1); // No new entry added
    }

    #[test]
    fn test_dedupe_clear() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));

        // Add some entries
        for i in 0..5 {
            let path = PathBuf::from(format!("/path/{}", i));
            cache.record(path, 1234, AccessResponse::Allow);
        }
        assert_eq!(cache.len(), 5);

        // Clear should remove all
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_dedupe_clear_expired() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(5));

        // Add entries
        for i in 0..3 {
            let path = PathBuf::from(format!("/path/{}", i));
            cache.record(path, 1234, AccessResponse::Allow);
        }
        assert_eq!(cache.len(), 3);

        // Wait for expiration
        thread::sleep(Duration::from_millis(10));

        // Clear expired should remove all
        cache.clear_expired();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_dedupe_mixed_expiration() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(20));

        // Add first entry
        cache.record(PathBuf::from("/path/old"), 1234, AccessResponse::Allow);

        // Wait a bit
        thread::sleep(Duration::from_millis(15));

        // Add second entry
        cache.record(PathBuf::from("/path/new"), 1234, AccessResponse::Allow);

        assert_eq!(cache.len(), 2);

        // Wait for first to expire but not second
        thread::sleep(Duration::from_millis(10));

        // Clear expired
        cache.clear_expired();

        // First should be expired, second should remain
        assert_eq!(cache.len(), 1);

        // First should be processed (expired)
        assert!(cache.should_process(Path::new("/path/old"), 1234, AccessResponse::Allow));

        // Second should still be a duplicate
        assert!(!cache.should_process(Path::new("/path/new"), 1234, AccessResponse::Allow));
    }

    #[test]
    fn test_dedupe_audit_and_deny_separate() {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let path = Path::new("/etc/passwd");

        // Audit event
        assert!(cache.check_and_record(path, 1234, AccessResponse::Audit));

        // Deny event for same path/pid should be separate
        assert!(cache.check_and_record(path, 1234, AccessResponse::Deny));

        // Allow event for same path/pid should be separate
        assert!(cache.check_and_record(path, 1234, AccessResponse::Allow));

        assert_eq!(cache.len(), 3);
    }
}
