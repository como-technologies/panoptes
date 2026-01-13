// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Guard Metrics Collection
//!
//! This module provides thread-safe metrics collection for fanotify guards.
//! Metrics are tracked per-guard and aggregated for daemon-wide reporting.
//!
//! ## Design
//!
//! - Uses `AtomicU64` for lock-free, high-frequency counter updates
//! - Per-guard metrics track allowed, denied, and audited events by type
//! - Aggregate metrics provide daemon-wide totals
//!
//! ## Security Considerations
//!
//! - Metrics do not contain sensitive path or process information
//! - Counter overflow is handled gracefully (wraps at u64::MAX)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use panoptes_common::{DaemonMetrics, MetricsSnapshot as CommonMetricsSnapshot};

/// Relaxed ordering for metric counters (eventual consistency is acceptable)
const ORDERING: Ordering = Ordering::Relaxed;

/// Metrics for a single fanotify guard.
///
/// All counters use atomic operations for thread-safe updates
/// from the guard's event loop without locking.
///
/// Implements the common `DaemonMetrics` trait while providing additional
/// fanotify-specific metrics like allowed/denied/audited counts.
#[derive(Debug)]
pub struct GuardMetrics {
    /// Name of this guard (for DaemonMetrics trait)
    guard_name: String,

    /// Total events processed by this guard
    pub events_total: AtomicU64,

    /// Events that were allowed
    pub events_allowed: AtomicU64,

    /// Events that were denied
    pub events_denied: AtomicU64,

    /// Events that were audited (logged to kernel audit)
    pub events_audited: AtomicU64,

    /// Access events (FAN_ACCESS)
    pub access_events: AtomicU64,

    /// Open events (FAN_OPEN)
    pub open_events: AtomicU64,

    /// Open for execution events (FAN_OPEN_EXEC)
    pub open_exec_events: AtomicU64,

    /// Close with write events (FAN_CLOSE_WRITE)
    pub close_write_events: AtomicU64,

    /// Close events (FAN_CLOSE)
    pub close_events: AtomicU64,

    /// Number of errors encountered
    pub errors: AtomicU64,

    /// Events deduplicated (not processed due to deduplication)
    pub deduplicated: AtomicU64,

    /// Policy cache hits
    pub policy_cache_hits: AtomicU64,

    /// Policy cache misses
    pub policy_cache_misses: AtomicU64,
}

impl GuardMetrics {
    /// Create a new metrics instance for a guard.
    pub fn new(guard_name: impl Into<String>) -> Self {
        Self {
            guard_name: guard_name.into(),
            events_total: AtomicU64::new(0),
            events_allowed: AtomicU64::new(0),
            events_denied: AtomicU64::new(0),
            events_audited: AtomicU64::new(0),
            access_events: AtomicU64::new(0),
            open_events: AtomicU64::new(0),
            open_exec_events: AtomicU64::new(0),
            close_write_events: AtomicU64::new(0),
            close_events: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            deduplicated: AtomicU64::new(0),
            policy_cache_hits: AtomicU64::new(0),
            policy_cache_misses: AtomicU64::new(0),
        }
    }

    /// Record an allowed event.
    #[inline]
    pub fn record_allowed(&self) {
        self.events_total.fetch_add(1, ORDERING);
        self.events_allowed.fetch_add(1, ORDERING);
    }

    /// Record a denied event.
    #[inline]
    pub fn record_denied(&self) {
        self.events_total.fetch_add(1, ORDERING);
        self.events_denied.fetch_add(1, ORDERING);
    }

    /// Record an audited event (logged but allowed).
    #[inline]
    pub fn record_audited(&self) {
        self.events_total.fetch_add(1, ORDERING);
        self.events_audited.fetch_add(1, ORDERING);
    }

    /// Record an event by type.
    #[inline]
    pub fn record_event_type(&self, event_type: &str) {
        match event_type {
            "access" => self.access_events.fetch_add(1, ORDERING),
            "open" => self.open_events.fetch_add(1, ORDERING),
            "open_exec" => self.open_exec_events.fetch_add(1, ORDERING),
            "close_write" => self.close_write_events.fetch_add(1, ORDERING),
            "close" | "close_nowrite" => self.close_events.fetch_add(1, ORDERING),
            _ => 0,
        };
    }

    /// Record an error.
    #[inline]
    #[allow(dead_code)]
    pub fn record_error(&self) {
        self.errors.fetch_add(1, ORDERING);
    }

    /// Record a deduplicated event.
    #[inline]
    #[allow(dead_code)]
    pub fn record_deduplicated(&self) {
        self.deduplicated.fetch_add(1, ORDERING);
    }

    /// Record a policy cache hit.
    #[inline]
    #[allow(dead_code)]
    pub fn record_policy_cache_hit(&self) {
        self.policy_cache_hits.fetch_add(1, ORDERING);
    }

    /// Record a policy cache miss.
    #[inline]
    #[allow(dead_code)]
    pub fn record_policy_cache_miss(&self) {
        self.policy_cache_misses.fetch_add(1, ORDERING);
    }

    /// Take a snapshot of current metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            events_total: self.events_total.load(ORDERING),
            events_allowed: self.events_allowed.load(ORDERING),
            events_denied: self.events_denied.load(ORDERING),
            events_audited: self.events_audited.load(ORDERING),
            access_events: self.access_events.load(ORDERING),
            open_events: self.open_events.load(ORDERING),
            open_exec_events: self.open_exec_events.load(ORDERING),
            close_write_events: self.close_write_events.load(ORDERING),
            close_events: self.close_events.load(ORDERING),
            errors: self.errors.load(ORDERING),
            deduplicated: self.deduplicated.load(ORDERING),
            policy_cache_hits: self.policy_cache_hits.load(ORDERING),
            policy_cache_misses: self.policy_cache_misses.load(ORDERING),
        }
    }

    /// Reset all counters to zero.
    #[allow(dead_code)]
    pub fn reset(&self) {
        self.events_total.store(0, ORDERING);
        self.events_allowed.store(0, ORDERING);
        self.events_denied.store(0, ORDERING);
        self.events_audited.store(0, ORDERING);
        self.access_events.store(0, ORDERING);
        self.open_events.store(0, ORDERING);
        self.open_exec_events.store(0, ORDERING);
        self.close_write_events.store(0, ORDERING);
        self.close_events.store(0, ORDERING);
        self.errors.store(0, ORDERING);
        self.deduplicated.store(0, ORDERING);
        self.policy_cache_hits.store(0, ORDERING);
        self.policy_cache_misses.store(0, ORDERING);
    }
}

impl Default for GuardMetrics {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Implement the common DaemonMetrics trait for MetricsAggregator integration.
impl DaemonMetrics for GuardMetrics {
    fn name(&self) -> &str {
        &self.guard_name
    }

    fn events_total(&self) -> u64 {
        self.events_total.load(ORDERING)
    }

    fn errors_total(&self) -> u64 {
        self.errors.load(ORDERING)
    }

    fn record_event(&self) {
        self.events_total.fetch_add(1, ORDERING);
    }

    fn record_error(&self) {
        self.errors.fetch_add(1, ORDERING);
    }

    fn snapshot(&self) -> CommonMetricsSnapshot {
        // Build custom metrics map with fanotify-specific data
        let mut custom = HashMap::new();

        // Add fanotify-specific metrics
        custom.insert("events_allowed".to_string(), self.events_allowed.load(ORDERING));
        custom.insert("events_denied".to_string(), self.events_denied.load(ORDERING));
        custom.insert("events_audited".to_string(), self.events_audited.load(ORDERING));
        custom.insert("access_events".to_string(), self.access_events.load(ORDERING));
        custom.insert("open_events".to_string(), self.open_events.load(ORDERING));
        custom.insert("open_exec_events".to_string(), self.open_exec_events.load(ORDERING));
        custom.insert("close_write_events".to_string(), self.close_write_events.load(ORDERING));
        custom.insert("close_events".to_string(), self.close_events.load(ORDERING));
        custom.insert("deduplicated".to_string(), self.deduplicated.load(ORDERING));
        custom.insert("policy_cache_hits".to_string(), self.policy_cache_hits.load(ORDERING));
        custom.insert("policy_cache_misses".to_string(), self.policy_cache_misses.load(ORDERING));

        CommonMetricsSnapshot {
            name: self.guard_name.clone(),
            events_total: self.events_total.load(ORDERING),
            errors_total: self.errors.load(ORDERING),
            custom,
        }
    }
}

/// Point-in-time snapshot of guard metrics.
///
/// Used for reporting and comparison without holding locks.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MetricsSnapshot {
    pub events_total: u64,
    pub events_allowed: u64,
    pub events_denied: u64,
    pub events_audited: u64,
    pub access_events: u64,
    pub open_events: u64,
    pub open_exec_events: u64,
    pub close_write_events: u64,
    pub close_events: u64,
    pub errors: u64,
    pub deduplicated: u64,
    pub policy_cache_hits: u64,
    pub policy_cache_misses: u64,
}

impl MetricsSnapshot {
    /// Get event counts as a map keyed by event type name.
    pub fn event_counts(&self) -> HashMap<String, i64> {
        let mut counts = HashMap::new();
        counts.insert("access".to_string(), self.access_events as i64);
        counts.insert("open".to_string(), self.open_events as i64);
        counts.insert("open_exec".to_string(), self.open_exec_events as i64);
        counts.insert("close_write".to_string(), self.close_write_events as i64);
        counts.insert("close".to_string(), self.close_events as i64);
        counts
    }
}

// Note: AggregateMetrics has been replaced by MetricsAggregator from panoptes-common.
// The common MetricsAggregator provides generic session counting and metrics collection.
// Fanotify-specific aggregation (allowed/denied/audited totals) is now computed
// on-demand in the get_metrics RPC handler by iterating per-guard metrics.

#[cfg(test)]
mod tests {
    use super::*;
    use panoptes_common::MetricsAggregator;
    use std::sync::Arc;

    #[test]
    fn test_guard_metrics_new() {
        let metrics = GuardMetrics::new("test-guard");
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.events_total, 0);
        assert_eq!(snapshot.events_allowed, 0);
        assert_eq!(snapshot.events_denied, 0);
        assert_eq!(snapshot.events_audited, 0);
    }

    #[test]
    fn test_guard_metrics_record_allowed() {
        let metrics = GuardMetrics::new("test");

        metrics.record_allowed();
        metrics.record_allowed();
        metrics.record_allowed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 3);
        assert_eq!(snapshot.events_allowed, 3);
        assert_eq!(snapshot.events_denied, 0);
    }

    #[test]
    fn test_guard_metrics_record_denied() {
        let metrics = GuardMetrics::new("test");

        metrics.record_denied();
        metrics.record_denied();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 2);
        assert_eq!(snapshot.events_denied, 2);
        assert_eq!(snapshot.events_allowed, 0);
    }

    #[test]
    fn test_guard_metrics_record_audited() {
        let metrics = GuardMetrics::new("test");

        metrics.record_audited();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 1);
        assert_eq!(snapshot.events_audited, 1);
    }

    #[test]
    fn test_guard_metrics_record_event_type() {
        let metrics = GuardMetrics::new("test");

        metrics.record_event_type("access");
        metrics.record_event_type("open");
        metrics.record_event_type("open");
        metrics.record_event_type("close_write");
        metrics.record_event_type("close");
        metrics.record_event_type("unknown");

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.access_events, 1);
        assert_eq!(snapshot.open_events, 2);
        assert_eq!(snapshot.close_write_events, 1);
        assert_eq!(snapshot.close_events, 1);
    }

    #[test]
    fn test_guard_metrics_record_error() {
        let metrics = GuardMetrics::new("test");

        metrics.record_error();
        metrics.record_error();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.errors, 2);
    }

    #[test]
    fn test_guard_metrics_record_deduplicated() {
        let metrics = GuardMetrics::new("test");

        metrics.record_deduplicated();
        metrics.record_deduplicated();
        metrics.record_deduplicated();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.deduplicated, 3);
    }

    #[test]
    fn test_guard_metrics_policy_cache() {
        let metrics = GuardMetrics::new("test");

        metrics.record_policy_cache_hit();
        metrics.record_policy_cache_hit();
        metrics.record_policy_cache_miss();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.policy_cache_hits, 2);
        assert_eq!(snapshot.policy_cache_misses, 1);
    }

    #[test]
    fn test_guard_metrics_reset() {
        let metrics = GuardMetrics::new("test");

        metrics.record_allowed();
        metrics.record_denied();
        metrics.record_error();

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 0);
        assert_eq!(snapshot.events_allowed, 0);
        assert_eq!(snapshot.events_denied, 0);
        assert_eq!(snapshot.errors, 0);
    }

    #[test]
    fn test_metrics_snapshot_event_counts() {
        let metrics = GuardMetrics::new("test");

        metrics.record_event_type("access");
        metrics.record_event_type("access");
        metrics.record_event_type("open");

        let snapshot = metrics.snapshot();
        let counts = snapshot.event_counts();

        assert_eq!(counts.get("access"), Some(&2));
        assert_eq!(counts.get("open"), Some(&1));
        assert_eq!(counts.get("close"), Some(&0));
    }

    #[test]
    fn test_daemon_metrics_trait() {
        let metrics = GuardMetrics::new("trait-test");
        let collector: &dyn DaemonMetrics = &metrics;

        assert_eq!(collector.name(), "trait-test");

        metrics.record_allowed();
        assert_eq!(collector.events_total(), 1);

        metrics.record_error();
        assert_eq!(collector.errors_total(), 1);
    }

    #[tokio::test]
    async fn test_metrics_aggregator_integration() {
        let aggregator = MetricsAggregator::new("test-node");

        // Create typed metrics first to call fanotify-specific methods
        let gm1 = Arc::new(GuardMetrics::new("guard-1"));
        let gm2 = Arc::new(GuardMetrics::new("guard-2"));

        gm1.record_allowed();
        gm1.record_denied();

        gm2.record_allowed();
        gm2.record_allowed();
        gm2.record_audited();

        // Cast to trait objects for registration
        let m1: Arc<dyn DaemonMetrics> = gm1;
        let m2: Arc<dyn DaemonMetrics> = gm2;

        aggregator.register(m1).await;
        aggregator.register(m2).await;

        // Verify active sessions tracked by aggregator
        let totals = aggregator.totals();
        assert_eq!(totals.active_sessions, 2);

        // Collect individual snapshots to verify session metrics
        let snapshots = aggregator.collect_all().await;
        assert_eq!(snapshots.len(), 2);

        let total_events: u64 = snapshots.iter().map(|s| s.events_total).sum();
        assert_eq!(total_events, 5); // 2 from guard-1, 3 from guard-2
    }

    #[test]
    fn test_metrics_thread_safety() {
        use std::thread;

        let metrics = Arc::new(GuardMetrics::new("concurrent"));
        let mut handles = vec![];

        for _ in 0..10 {
            let m = Arc::clone(&metrics);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    m.record_allowed();
                    m.record_event_type("access");
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_total, 10_000);
        assert_eq!(snapshot.events_allowed, 10_000);
        assert_eq!(snapshot.access_events, 10_000);
    }
}
