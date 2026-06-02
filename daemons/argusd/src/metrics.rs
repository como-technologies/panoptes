// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Metrics Collection for Argusd
//!
//! This module provides inotify-specific metrics tracking that extends the
//! common `DaemonMetrics` trait from panoptes-common.
//!
//! ## Metrics Collected
//!
//! - `events_total` - Total events processed by type
//! - `watches_active` - Number of active watch descriptors
//! - `watches_total` - Total watches created
//! - `errors_total` - Total errors by type
//! - `queue_overflows` - Number of `IN_Q_OVERFLOW` events
//! - `move_pairs_matched` - Successfully matched rename events
//! - `move_pairs_timeout` - Rename events that timed out waiting for pair
//!
//! ## Usage
//!
//! ```rust,no_run
//! use argusd::metrics::WatcherMetrics;
//! use panoptes_common::DaemonMetrics;
//!
//! let metrics = WatcherMetrics::new("my-watcher");
//! metrics.record_event();
//! metrics.record_event_typed("create");
//!
//! let snapshot = metrics.snapshot();
//! println!("Total events: {}", snapshot.events_total);
//! ```

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use panoptes_common::{DaemonMetrics, MetricsSnapshot};

/// Per-watcher metrics collector with inotify-specific extensions.
///
/// Implements the common `DaemonMetrics` trait while providing additional
/// inotify-specific metrics like watch descriptor counts and queue overflows.
///
/// Thread-safe metrics collection using atomic counters for high-frequency
/// updates without lock contention.
pub struct WatcherMetrics {
    watcher_name: String,
    events_total: AtomicU64,
    events_by_type: RwLock<HashMap<String, AtomicU64>>,
    watches_active: AtomicU64,
    watches_created: AtomicU64,
    watches_removed: AtomicU64,
    errors_total: AtomicU64,
    queue_overflows: AtomicU64,
    move_pairs_matched: AtomicU64,
    move_pairs_timeout: AtomicU64,
    start_time: Instant,
}

impl WatcherMetrics {
    /// Create a new metrics collector for a watcher.
    pub fn new(watcher_name: impl Into<String>) -> Self {
        Self {
            watcher_name: watcher_name.into(),
            events_total: AtomicU64::new(0),
            events_by_type: RwLock::new(HashMap::new()),
            watches_active: AtomicU64::new(0),
            watches_created: AtomicU64::new(0),
            watches_removed: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            queue_overflows: AtomicU64::new(0),
            move_pairs_matched: AtomicU64::new(0),
            move_pairs_timeout: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Record a file system event with its type.
    ///
    /// This extends the basic `record_event()` from DaemonMetrics trait
    /// by also tracking events by type.
    pub fn record_event_typed(&self, event_type: &str) {
        self.events_total.fetch_add(1, Ordering::Relaxed);

        // Update per-type counter
        {
            let types = self.events_by_type.read().unwrap();
            if let Some(counter) = types.get(event_type) {
                counter.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Need to create new counter
        {
            let mut types = self.events_by_type.write().unwrap();
            types
                .entry(event_type.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    // Inotify-specific methods (used by notify.rs which is always compiled)

    /// Record a watch being added.
    pub fn record_watch_added(&self) {
        self.watches_active.fetch_add(1, Ordering::Relaxed);
        self.watches_created.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a watch being removed.
    pub fn record_watch_removed(&self) {
        self.watches_active.fetch_sub(1, Ordering::Relaxed);
        self.watches_removed.fetch_add(1, Ordering::Relaxed);
    }

    /// Set the current number of active watches.
    pub fn set_watches_active(&self, count: u64) {
        self.watches_active.store(count, Ordering::Relaxed);
    }

    /// Record a queue overflow event (IN_Q_OVERFLOW).
    pub fn record_queue_overflow(&self) {
        self.queue_overflows.fetch_add(1, Ordering::Relaxed);
        self.record_event_typed("overflow");
    }

    /// Record a successfully matched move pair.
    pub fn record_move_pair_matched(&self) {
        self.move_pairs_matched.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a move event that timed out.
    pub fn record_move_pair_timeout(&self) {
        self.move_pairs_timeout.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current watches active count.
    #[allow(dead_code)]
    pub fn watches_active(&self) -> u64 {
        self.watches_active.load(Ordering::Relaxed)
    }

    /// Reset all counters.
    #[allow(dead_code)]
    pub fn reset(&self) {
        self.events_total.store(0, Ordering::Relaxed);
        self.events_by_type.write().unwrap().clear();
        self.watches_active.store(0, Ordering::Relaxed);
        self.watches_created.store(0, Ordering::Relaxed);
        self.watches_removed.store(0, Ordering::Relaxed);
        self.errors_total.store(0, Ordering::Relaxed);
        self.queue_overflows.store(0, Ordering::Relaxed);
        self.move_pairs_matched.store(0, Ordering::Relaxed);
        self.move_pairs_timeout.store(0, Ordering::Relaxed);
    }
}

impl Default for WatcherMetrics {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Implement the common DaemonMetrics trait.
impl DaemonMetrics for WatcherMetrics {
    fn name(&self) -> &str {
        &self.watcher_name
    }

    fn events_total(&self) -> u64 {
        self.events_total.load(Ordering::Relaxed)
    }

    fn errors_total(&self) -> u64 {
        self.errors_total.load(Ordering::Relaxed)
    }

    fn record_event(&self) {
        self.events_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_error(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> MetricsSnapshot {
        // Build custom metrics map with inotify-specific data
        let mut custom = HashMap::new();

        // Add event counts by type
        {
            let types = self.events_by_type.read().unwrap();
            for (event_type, count) in types.iter() {
                custom.insert(event_type.clone(), count.load(Ordering::Relaxed));
            }
        }

        // Add inotify-specific metrics
        custom.insert(
            "watches_active".to_string(),
            self.watches_active.load(Ordering::Relaxed),
        );
        custom.insert(
            "watches_created".to_string(),
            self.watches_created.load(Ordering::Relaxed),
        );
        custom.insert(
            "watches_removed".to_string(),
            self.watches_removed.load(Ordering::Relaxed),
        );
        custom.insert(
            "queue_overflows".to_string(),
            self.queue_overflows.load(Ordering::Relaxed),
        );
        custom.insert(
            "move_pairs_matched".to_string(),
            self.move_pairs_matched.load(Ordering::Relaxed),
        );
        custom.insert(
            "move_pairs_timeout".to_string(),
            self.move_pairs_timeout.load(Ordering::Relaxed),
        );
        custom.insert(
            "uptime_seconds".to_string(),
            self.start_time.elapsed().as_secs(),
        );

        MetricsSnapshot {
            name: self.watcher_name.clone(),
            events_total: self.events_total.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
            custom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panoptes_common::MetricsAggregator;
    use std::sync::Arc;

    #[test]
    fn test_watcher_metrics_creation() {
        let metrics = WatcherMetrics::new("test-watcher");
        assert_eq!(metrics.watcher_name, "test-watcher");

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 0);
        assert_eq!(snapshot.custom.get("watches_active"), Some(&0));
    }

    #[test]
    fn test_record_events_typed() {
        let metrics = WatcherMetrics::new("test");

        metrics.record_event_typed("create");
        metrics.record_event_typed("create");
        metrics.record_event_typed("modify");

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 3);
        assert_eq!(snapshot.custom.get("create"), Some(&2));
        assert_eq!(snapshot.custom.get("modify"), Some(&1));
    }

    #[test]
    fn test_watch_tracking() {
        let metrics = WatcherMetrics::new("test");

        metrics.record_watch_added();
        metrics.record_watch_added();
        metrics.record_watch_added();
        metrics.record_watch_removed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.custom.get("watches_active"), Some(&2));
        assert_eq!(snapshot.custom.get("watches_created"), Some(&3));
        assert_eq!(snapshot.custom.get("watches_removed"), Some(&1));
    }

    #[test]
    fn test_move_pair_tracking() {
        let metrics = WatcherMetrics::new("test");

        metrics.record_move_pair_matched();
        metrics.record_move_pair_matched();
        metrics.record_move_pair_timeout();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.custom.get("move_pairs_matched"), Some(&2));
        assert_eq!(snapshot.custom.get("move_pairs_timeout"), Some(&1));
    }

    #[test]
    fn test_queue_overflow() {
        let metrics = WatcherMetrics::new("test");

        metrics.record_queue_overflow();
        metrics.record_queue_overflow();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.custom.get("queue_overflows"), Some(&2));
        // overflow also records as an event type
        assert_eq!(snapshot.custom.get("overflow"), Some(&2));
    }

    #[test]
    fn test_reset() {
        let metrics = WatcherMetrics::new("test");

        metrics.record_event_typed("create");
        metrics.record_watch_added();
        metrics.record_error();

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 0);
        assert_eq!(snapshot.custom.get("watches_active"), Some(&0));
        assert_eq!(snapshot.errors_total, 0);
    }

    #[test]
    fn test_daemon_metrics_trait() {
        let metrics = WatcherMetrics::new("trait-test");
        let collector: &dyn DaemonMetrics = &metrics;

        assert_eq!(collector.name(), "trait-test");

        metrics.record_event_typed("test");
        assert_eq!(collector.events_total(), 1);
    }

    #[tokio::test]
    async fn test_metrics_aggregator_integration() {
        let aggregator = MetricsAggregator::new("test-node");

        // Create typed metrics first to call typed methods
        let wm1 = Arc::new(WatcherMetrics::new("watcher-1"));
        let wm2 = Arc::new(WatcherMetrics::new("watcher-2"));

        wm1.record_event_typed("create");
        wm1.record_watch_added();

        wm2.record_event_typed("modify");
        wm2.record_watch_added();
        wm2.record_watch_added();

        // Cast to trait objects for registration
        let m1: Arc<dyn DaemonMetrics> = wm1;
        let m2: Arc<dyn DaemonMetrics> = wm2;

        aggregator.register(m1).await;
        aggregator.register(m2).await;

        // Verify active sessions tracked by aggregator
        let totals = aggregator.totals();
        assert_eq!(totals.active_sessions, 2);

        // Collect individual snapshots to verify session metrics
        let snapshots = aggregator.collect_all().await;
        assert_eq!(snapshots.len(), 2);

        let total_events: u64 = snapshots.iter().map(|s| s.events_total).sum();
        assert_eq!(total_events, 2);
    }

    #[test]
    fn test_concurrent_updates() {
        use std::thread;

        let metrics = Arc::new(WatcherMetrics::new("concurrent"));
        let mut handles = vec![];

        for _ in 0..10 {
            let m = Arc::clone(&metrics);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    m.record_event_typed("create");
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 1000);
    }
}
