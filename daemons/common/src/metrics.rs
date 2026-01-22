//! # Metrics Framework
//!
//! Generic metrics collection for Panoptes daemons.
//!
//! This module provides:
//! - `DaemonMetrics` trait - interface for per-session metrics
//! - `MetricsSnapshot` - point-in-time capture of metrics
//! - `MetricsAggregator` - aggregate metrics across all sessions
//!
//! ## Design
//!
//! Metrics use atomic counters for lock-free updates on hot paths.
//! Snapshots are collected on-demand for reporting without blocking updates.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

/// Point-in-time snapshot of metrics.
#[derive(Clone, Debug, Default)]
pub struct MetricsSnapshot {
    /// Name of the session/collector
    pub name: String,

    /// Total events processed
    pub events_total: u64,

    /// Total errors encountered
    pub errors_total: u64,

    /// Additional daemon-specific metrics
    pub custom: HashMap<String, u64>,
}

impl MetricsSnapshot {
    /// Create a new snapshot with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Add a custom metric value.
    pub fn with_custom(mut self, key: impl Into<String>, value: u64) -> Self {
        self.custom.insert(key.into(), value);
        self
    }
}

/// Trait for per-session metrics collection.
///
/// Implement this trait to provide custom metrics for your daemon.
/// The trait uses atomic operations for thread-safe, lock-free updates.
///
/// # Example
///
/// ```rust,ignore
/// pub struct WatcherMetrics {
///     name: String,
///     events_total: AtomicU64,
///     errors_total: AtomicU64,
///     watches_active: AtomicU64,
/// }
///
/// impl DaemonMetrics for WatcherMetrics {
///     fn name(&self) -> &str { &self.name }
///     fn events_total(&self) -> u64 { self.events_total.load(Ordering::Relaxed) }
///     fn errors_total(&self) -> u64 { self.errors_total.load(Ordering::Relaxed) }
///     fn record_event(&self) { self.events_total.fetch_add(1, Ordering::Relaxed); }
///     fn record_error(&self) { self.errors_total.fetch_add(1, Ordering::Relaxed); }
///     fn snapshot(&self) -> MetricsSnapshot { ... }
/// }
/// ```
pub trait DaemonMetrics: Send + Sync {
    /// Get the name of this metrics collector (usually the session name).
    fn name(&self) -> &str;

    /// Get the total number of events processed.
    fn events_total(&self) -> u64;

    /// Get the total number of errors encountered.
    fn errors_total(&self) -> u64;

    /// Record a successful event.
    fn record_event(&self);

    /// Record an error.
    fn record_error(&self);

    /// Capture a point-in-time snapshot of all metrics.
    fn snapshot(&self) -> MetricsSnapshot;
}

/// Basic metrics implementation using atomic counters.
///
/// This provides a simple implementation of `DaemonMetrics` that can be
/// used directly or as a base for more specialized metrics.
pub struct BasicMetrics {
    name: String,
    events_total: AtomicU64,
    errors_total: AtomicU64,
}

impl BasicMetrics {
    /// Create new basic metrics with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
        }
    }
}

impl DaemonMetrics for BasicMetrics {
    fn name(&self) -> &str {
        &self.name
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
        MetricsSnapshot {
            name: self.name.clone(),
            events_total: self.events_total(),
            errors_total: self.errors_total(),
            custom: HashMap::new(),
        }
    }
}

/// Aggregates metrics across all sessions on a node.
///
/// Maintains both aggregate totals (using atomics for lock-free updates)
/// and per-session snapshots (using RwLock for concurrent reads).
pub struct MetricsAggregator {
    /// Node this aggregator is running on
    node_name: String,

    /// Registered metrics collectors
    collectors: RwLock<Vec<Arc<dyn DaemonMetrics>>>,

    /// Aggregate event count
    total_events: AtomicU64,

    /// Aggregate error count
    total_errors: AtomicU64,

    /// Active session count
    active_sessions: AtomicU64,
}

impl MetricsAggregator {
    /// Create a new metrics aggregator for the given node.
    pub fn new(node_name: impl Into<String>) -> Self {
        Self {
            node_name: node_name.into(),
            collectors: RwLock::new(Vec::new()),
            total_events: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            active_sessions: AtomicU64::new(0),
        }
    }

    /// Get the node name.
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    /// Register a metrics collector for a new session.
    pub async fn register(&self, collector: Arc<dyn DaemonMetrics>) {
        self.collectors.write().await.push(collector);
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
    }

    /// Unregister a metrics collector when a session is destroyed.
    pub async fn unregister(&self, name: &str) {
        let mut collectors = self.collectors.write().await;
        if let Some(pos) = collectors.iter().position(|c| c.name() == name) {
            collectors.remove(pos);
            self.active_sessions.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Record an event in aggregate totals.
    pub fn record_event(&self) {
        self.total_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error in aggregate totals.
    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get aggregate totals.
    pub fn totals(&self) -> AggregateTotals {
        AggregateTotals {
            node_name: self.node_name.clone(),
            events_total: self.total_events.load(Ordering::Relaxed),
            errors_total: self.total_errors.load(Ordering::Relaxed),
            active_sessions: self.active_sessions.load(Ordering::Relaxed),
        }
    }

    /// Collect snapshots from all registered collectors.
    pub async fn collect_all(&self) -> Vec<MetricsSnapshot> {
        let collectors = self.collectors.read().await;
        collectors.iter().map(|c| c.snapshot()).collect()
    }

    /// Collect snapshots filtered by name prefix.
    pub async fn collect_filtered(&self, name_filter: Option<&str>) -> Vec<MetricsSnapshot> {
        let collectors = self.collectors.read().await;
        collectors
            .iter()
            .filter(|c| name_filter.map(|f| c.name().starts_with(f)).unwrap_or(true))
            .map(|c| c.snapshot())
            .collect()
    }

    /// Get the number of active sessions.
    pub fn active_sessions(&self) -> u64 {
        self.active_sessions.load(Ordering::Relaxed)
    }
}

/// Aggregate totals across all sessions.
#[derive(Clone, Debug)]
pub struct AggregateTotals {
    /// Node name
    pub node_name: String,

    /// Total events across all sessions
    pub events_total: u64,

    /// Total errors across all sessions
    pub errors_total: u64,

    /// Number of active sessions
    pub active_sessions: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_metrics_creation() {
        let metrics = BasicMetrics::new("test-session");
        assert_eq!(metrics.name(), "test-session");
        assert_eq!(metrics.events_total(), 0);
        assert_eq!(metrics.errors_total(), 0);
    }

    #[test]
    fn test_basic_metrics_recording() {
        let metrics = BasicMetrics::new("test");

        metrics.record_event();
        metrics.record_event();
        metrics.record_error();

        assert_eq!(metrics.events_total(), 2);
        assert_eq!(metrics.errors_total(), 1);
    }

    #[test]
    fn test_basic_metrics_snapshot() {
        let metrics = BasicMetrics::new("test");
        metrics.record_event();
        metrics.record_error();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.name, "test");
        assert_eq!(snapshot.events_total, 1);
        assert_eq!(snapshot.errors_total, 1);
    }

    #[test]
    fn test_metrics_snapshot_with_custom() {
        let snapshot = MetricsSnapshot::new("test")
            .with_custom("watches_active", 5)
            .with_custom("events_by_type", 100);

        assert_eq!(snapshot.custom.get("watches_active"), Some(&5));
        assert_eq!(snapshot.custom.get("events_by_type"), Some(&100));
    }

    #[tokio::test]
    async fn test_aggregator_creation() {
        let agg = MetricsAggregator::new("node1");
        assert_eq!(agg.node_name(), "node1");
        assert_eq!(agg.active_sessions(), 0);
    }

    #[tokio::test]
    async fn test_aggregator_register_unregister() {
        let agg = MetricsAggregator::new("node1");

        let metrics1 = Arc::new(BasicMetrics::new("session1"));
        let metrics2 = Arc::new(BasicMetrics::new("session2"));

        agg.register(metrics1).await;
        assert_eq!(agg.active_sessions(), 1);

        agg.register(metrics2).await;
        assert_eq!(agg.active_sessions(), 2);

        agg.unregister("session1").await;
        assert_eq!(agg.active_sessions(), 1);
    }

    #[tokio::test]
    async fn test_aggregator_totals() {
        let agg = MetricsAggregator::new("node1");

        agg.record_event();
        agg.record_event();
        agg.record_error();

        let totals = agg.totals();
        assert_eq!(totals.events_total, 2);
        assert_eq!(totals.errors_total, 1);
    }

    #[tokio::test]
    async fn test_aggregator_collect_all() {
        let agg = MetricsAggregator::new("node1");

        let metrics1 = Arc::new(BasicMetrics::new("session1"));
        metrics1.record_event();

        let metrics2 = Arc::new(BasicMetrics::new("session2"));
        metrics2.record_event();
        metrics2.record_event();

        agg.register(metrics1).await;
        agg.register(metrics2).await;

        let snapshots = agg.collect_all().await;
        assert_eq!(snapshots.len(), 2);

        let total_events: u64 = snapshots.iter().map(|s| s.events_total).sum();
        assert_eq!(total_events, 3);
    }

    #[tokio::test]
    async fn test_aggregator_collect_filtered() {
        let agg = MetricsAggregator::new("node1");

        agg.register(Arc::new(BasicMetrics::new("watcher-foo")))
            .await;
        agg.register(Arc::new(BasicMetrics::new("watcher-bar")))
            .await;
        agg.register(Arc::new(BasicMetrics::new("other-baz"))).await;

        let filtered = agg.collect_filtered(Some("watcher")).await;
        assert_eq!(filtered.len(), 2);

        let all = agg.collect_filtered(None).await;
        assert_eq!(all.len(), 3);
    }
}
