//! # Event Broadcasting
//!
//! Generic event broadcasting for Panoptes daemons.
//!
//! This module provides:
//! - `EventBroadcaster<E>` - broadcast channel wrapper for multi-client streaming
//! - `StreamFilter` - configurable event filtering
//! - `Filterable` trait - interface for filterable events
//!
//! ## Design
//!
//! Uses tokio's broadcast channel which allows multiple independent receivers.
//! Each `StreamEvents` client gets their own receiver via `subscribe()`,
//! enabling independent consumption without blocking other clients.

use tokio::sync::broadcast;

/// Generic event broadcaster for multi-client streaming.
///
/// Wraps a tokio broadcast channel with a configurable capacity.
/// Each client can subscribe independently and receives all events
/// sent after subscription.
///
/// # Example
///
/// ```rust,ignore
/// let broadcaster = EventBroadcaster::<FileEvent>::new(10000);
///
/// // In event forwarding task:
/// broadcaster.send(event);
///
/// // In StreamEvents handler:
/// let mut rx = broadcaster.subscribe();
/// while let Ok(event) = rx.recv().await {
///     // process event
/// }
/// ```
pub struct EventBroadcaster<E: Clone + Send + 'static> {
    tx: broadcast::Sender<E>,
    capacity: usize,
}

impl<E: Clone + Send + 'static> Clone for EventBroadcaster<E> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            capacity: self.capacity,
        }
    }
}

impl<E: Clone + Send + 'static> EventBroadcaster<E> {
    /// Create a new broadcaster with the given capacity.
    ///
    /// Capacity determines how many events can be buffered before
    /// slow clients start lagging (missing events).
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, capacity }
    }

    /// Send an event to all subscribers.
    ///
    /// Returns the number of active receivers, or an error if there
    /// are no receivers (which is usually fine - events are just dropped).
    pub fn send(&self, event: E) -> Result<usize, broadcast::error::SendError<E>> {
        self.tx.send(event)
    }

    /// Subscribe to receive events.
    ///
    /// Returns a receiver that will get all events sent after this call.
    /// Each subscriber gets an independent copy of events.
    pub fn subscribe(&self) -> broadcast::Receiver<E> {
        self.tx.subscribe()
    }

    /// Get the capacity of this broadcaster.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the current number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl<E: Clone + Send + 'static> Default for EventBroadcaster<E> {
    fn default() -> Self {
        Self::new(10000)
    }
}

/// Filter configuration for stream subscriptions.
///
/// Used to filter events before sending to clients.
/// All conditions must match (AND logic).
#[derive(Clone, Debug, Default)]
pub struct StreamFilter {
    /// Filter by session/resource name (exact match)
    pub name: Option<String>,

    /// Filter by namespace (exact match)
    pub namespace: Option<String>,

    /// Filter by event types (any match)
    pub event_types: Vec<String>,
}

impl StreamFilter {
    /// Create a new empty filter (matches everything).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the name filter.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the namespace filter.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set the event types filter.
    pub fn with_event_types(mut self, types: Vec<String>) -> Self {
        self.event_types = types;
        self
    }

    /// Check if an event matches this filter.
    pub fn matches<E: Filterable>(&self, event: &E) -> bool {
        // Check name filter
        if let Some(ref name) = self.name {
            if event.filter_name() != name {
                return false;
            }
        }

        // Check namespace filter
        if let Some(ref ns) = self.namespace {
            if event.filter_namespace() != ns {
                return false;
            }
        }

        // Check event types filter (any match)
        if !self.event_types.is_empty() {
            if !self.event_types.contains(&event.filter_event_type()) {
                return false;
            }
        }

        true
    }

    /// Check if this filter has any conditions.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.namespace.is_none() && self.event_types.is_empty()
    }
}

/// Trait for events that can be filtered.
///
/// Implement this trait on your event types to enable filtering.
///
/// # Example
///
/// ```rust,ignore
/// impl Filterable for FileEvent {
///     fn filter_name(&self) -> &str { &self.watcher_name }
///     fn filter_namespace(&self) -> &str { &self.namespace }
///     fn filter_event_type(&self) -> String {
///         inotify_event_to_string(self.event_type)
///     }
/// }
/// ```
pub trait Filterable {
    /// Get the name for filtering (e.g., watcher_name, guard_name).
    fn filter_name(&self) -> &str;

    /// Get the namespace for filtering.
    fn filter_namespace(&self) -> &str;

    /// Get the event type as a string for filtering.
    fn filter_event_type(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test event type
    #[derive(Clone, Debug)]
    struct TestEvent {
        name: String,
        namespace: String,
        event_type: String,
        data: String,
    }

    impl Filterable for TestEvent {
        fn filter_name(&self) -> &str {
            &self.name
        }
        fn filter_namespace(&self) -> &str {
            &self.namespace
        }
        fn filter_event_type(&self) -> String {
            self.event_type.clone()
        }
    }

    fn test_event(name: &str, ns: &str, event_type: &str) -> TestEvent {
        TestEvent {
            name: name.to_string(),
            namespace: ns.to_string(),
            event_type: event_type.to_string(),
            data: "test".to_string(),
        }
    }

    #[test]
    fn test_broadcaster_creation() {
        let broadcaster = EventBroadcaster::<TestEvent>::new(1000);
        assert_eq!(broadcaster.capacity(), 1000);
        assert_eq!(broadcaster.receiver_count(), 0);
    }

    #[test]
    fn test_broadcaster_default() {
        let broadcaster = EventBroadcaster::<TestEvent>::default();
        assert_eq!(broadcaster.capacity(), 10000);
    }

    #[tokio::test]
    async fn test_broadcaster_send_receive() {
        let broadcaster = EventBroadcaster::<TestEvent>::new(100);
        let mut rx = broadcaster.subscribe();

        let event = test_event("test", "default", "modify");
        broadcaster.send(event.clone()).unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.name, "test");
    }

    #[tokio::test]
    async fn test_broadcaster_multiple_receivers() {
        let broadcaster = EventBroadcaster::<TestEvent>::new(100);
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();

        assert_eq!(broadcaster.receiver_count(), 2);

        let event = test_event("test", "default", "modify");
        broadcaster.send(event).unwrap();

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        assert_eq!(e1.name, e2.name);
    }

    #[test]
    fn test_filter_empty() {
        let filter = StreamFilter::new();
        assert!(filter.is_empty());

        let event = test_event("any", "any", "any");
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_filter_by_name() {
        let filter = StreamFilter::new().with_name("watcher1");

        let matching = test_event("watcher1", "default", "modify");
        let non_matching = test_event("watcher2", "default", "modify");

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn test_filter_by_namespace() {
        let filter = StreamFilter::new().with_namespace("prod");

        let matching = test_event("watcher1", "prod", "modify");
        let non_matching = test_event("watcher1", "dev", "modify");

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn test_filter_by_event_types() {
        let filter =
            StreamFilter::new().with_event_types(vec!["modify".to_string(), "create".to_string()]);

        let matching1 = test_event("w", "ns", "modify");
        let matching2 = test_event("w", "ns", "create");
        let non_matching = test_event("w", "ns", "delete");

        assert!(filter.matches(&matching1));
        assert!(filter.matches(&matching2));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn test_filter_combined() {
        let filter = StreamFilter::new()
            .with_name("watcher1")
            .with_namespace("prod")
            .with_event_types(vec!["modify".to_string()]);

        // All conditions match
        let matching = test_event("watcher1", "prod", "modify");
        assert!(filter.matches(&matching));

        // Name doesn't match
        let wrong_name = test_event("watcher2", "prod", "modify");
        assert!(!filter.matches(&wrong_name));

        // Namespace doesn't match
        let wrong_ns = test_event("watcher1", "dev", "modify");
        assert!(!filter.matches(&wrong_ns));

        // Event type doesn't match
        let wrong_type = test_event("watcher1", "prod", "delete");
        assert!(!filter.matches(&wrong_type));
    }
}
