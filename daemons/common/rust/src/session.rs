//! # Session Management
//!
//! Generic session management for Panoptes daemons.
//!
//! This module provides a type-safe, generic session container and management
//! trait that can be used by both argusd (watch sessions) and janusd (guard sessions).
//!
//! ## Architecture
//!
//! Sessions are stored in a nested lock structure for optimal concurrency:
//! - `RwLock<HashMap<...>>` - Allows concurrent reads of the session map
//! - `Arc<Mutex<Session>>` - Allows exclusive access to individual sessions
//!
//! This design enables:
//! - Multiple readers to enumerate sessions simultaneously
//! - Single writer for session create/destroy
//! - Independent locking of individual sessions for modification

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::{Mutex, RwLock};

use crate::metrics::DaemonMetrics;

/// Type alias for the session storage map.
///
/// Uses nested locks for optimal concurrency:
/// - Outer `RwLock` allows concurrent reads of the HashMap
/// - Inner `Arc<Mutex<>>` allows independent session access
pub type SessionMap<T> = Arc<RwLock<HashMap<String, Arc<Mutex<Session<T>>>>>>;

/// Trait for daemon-specific session state.
///
/// Implement this trait to define what additional state your daemon needs
/// beyond the common session fields.
///
/// # Example
///
/// ```rust,ignore
/// pub struct WatchState {
///     pub subjects: Vec<WatchSubject>,
///     pub watcher: Option<Watcher>,
/// }
///
/// impl SessionState for WatchState {
///     type Config = WatchSubject;
///     type Event = FileEvent;
/// }
/// ```
pub trait SessionState: Send + Sync + 'static {
    /// Configuration type for this session (e.g., WatchSubject, GuardSubject)
    type Config: Send + Sync;
    /// Event type this session produces (e.g., FileEvent, AccessEvent)
    type Event: Clone + Send + Sync;
}

/// Generic session container.
///
/// Holds both common fields shared by all daemons and daemon-specific
/// state via the generic `T: SessionState` parameter.
pub struct Session<T: SessionState> {
    /// Unique session identifier (format: "name:namespace:pod_name")
    pub id: String,

    /// Name of the watcher/guard resource
    pub name: String,

    /// Kubernetes namespace
    pub namespace: String,

    /// Node this session is running on
    pub node_name: String,

    /// Pod being monitored
    pub pod_name: String,

    /// Container IDs being monitored
    pub container_ids: Vec<String>,

    /// Resolved PIDs for the containers
    pub pids: Vec<i32>,

    /// Whether the session is paused
    pub paused: bool,

    /// When the session was created
    pub created_at: SystemTime,

    /// Atomic flag for signaling stop (shared with monitoring tasks)
    pub running: Arc<AtomicBool>,

    /// Per-session metrics collector
    pub metrics: Arc<dyn DaemonMetrics>,

    /// Daemon-specific state (WatchState or GuardState)
    pub state: T,
}

impl<T: SessionState> Session<T> {
    /// Create a new session with the given parameters.
    pub fn new(
        id: String,
        name: String,
        namespace: String,
        node_name: String,
        pod_name: String,
        container_ids: Vec<String>,
        pids: Vec<i32>,
        paused: bool,
        metrics: Arc<dyn DaemonMetrics>,
        state: T,
    ) -> Self {
        Self {
            id,
            name,
            namespace,
            node_name,
            pod_name,
            container_ids,
            pids,
            paused,
            created_at: SystemTime::now(),
            running: Arc::new(AtomicBool::new(true)),
            metrics,
            state,
        }
    }

    /// Signal this session to stop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if this session is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get a clone of the running flag for use in spawned tasks.
    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }
}

/// Trait for managing sessions.
///
/// Provides default implementations for common session operations.
/// Daemons implement this trait by providing access to their session map.
///
/// # Example
///
/// ```rust,ignore
/// impl SessionManager<WatchState> for ArgusdServiceImpl {
///     fn sessions(&self) -> &SessionMap<WatchState> {
///         &self.sessions
///     }
///
///     fn max_sessions(&self) -> usize {
///         self.max_watches
///     }
/// }
/// ```
#[allow(async_fn_in_trait)]
pub trait SessionManager<T: SessionState> {
    /// Get a reference to the session map.
    fn sessions(&self) -> &SessionMap<T>;

    /// Get the maximum number of sessions allowed.
    fn max_sessions(&self) -> usize;

    /// Generate a composite session key from name, namespace, and pod name.
    fn session_key(name: &str, namespace: &str, pod_name: &str) -> String {
        format!("{}:{}:{}", name, namespace, pod_name)
    }

    /// Check if a session exists.
    async fn exists(&self, key: &str) -> bool {
        self.sessions().read().await.contains_key(key)
    }

    /// Get a session by key.
    async fn get(&self, key: &str) -> Option<Arc<Mutex<Session<T>>>> {
        self.sessions().read().await.get(key).cloned()
    }

    /// Insert a new session.
    ///
    /// Returns an error if the session already exists or max sessions reached.
    async fn insert(
        &self,
        key: String,
        session: Session<T>,
    ) -> Result<Arc<Mutex<Session<T>>>, SessionError> {
        let mut sessions = self.sessions().write().await;

        if sessions.len() >= self.max_sessions() {
            return Err(SessionError::MaxSessionsReached {
                max: self.max_sessions(),
            });
        }

        if sessions.contains_key(&key) {
            return Err(SessionError::AlreadyExists { key });
        }

        let session = Arc::new(Mutex::new(session));
        sessions.insert(key, session.clone());
        Ok(session)
    }

    /// Remove a session and signal it to stop.
    ///
    /// Returns the removed session if it existed.
    async fn remove(&self, key: &str) -> Option<Arc<Mutex<Session<T>>>> {
        let session = self.sessions().write().await.remove(key);
        if let Some(ref s) = session {
            s.lock().await.stop();
        }
        session
    }

    /// Count active sessions.
    async fn count(&self) -> usize {
        self.sessions().read().await.len()
    }

    /// Get all session keys.
    async fn keys(&self) -> Vec<String> {
        self.sessions().read().await.keys().cloned().collect()
    }

    /// Check if we can add more sessions.
    async fn has_capacity(&self) -> bool {
        self.count().await < self.max_sessions()
    }
}

/// Errors that can occur during session management.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session already exists: {key}")]
    AlreadyExists { key: String },

    #[error("session not found: {key}")]
    NotFound { key: String },

    #[error("maximum sessions reached: {max}")]
    MaxSessionsReached { max: usize },
}

/// Create a new empty session map.
pub fn new_session_map<T: SessionState>() -> SessionMap<T> {
    Arc::new(RwLock::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test session state
    struct TestState {
        data: String,
    }

    impl SessionState for TestState {
        type Config = String;
        type Event = String;
    }

    // Mock metrics for testing
    struct MockMetrics {
        name: String,
    }

    impl DaemonMetrics for MockMetrics {
        fn name(&self) -> &str {
            &self.name
        }
        fn events_total(&self) -> u64 {
            0
        }
        fn errors_total(&self) -> u64 {
            0
        }
        fn record_event(&self) {}
        fn record_error(&self) {}
        fn snapshot(&self) -> crate::metrics::MetricsSnapshot {
            crate::metrics::MetricsSnapshot::default()
        }
    }

    // Test session manager
    struct TestManager {
        sessions: SessionMap<TestState>,
        max: usize,
    }

    impl SessionManager<TestState> for TestManager {
        fn sessions(&self) -> &SessionMap<TestState> {
            &self.sessions
        }
        fn max_sessions(&self) -> usize {
            self.max
        }
    }

    fn create_test_session(name: &str) -> Session<TestState> {
        Session::new(
            format!("{}:default:pod1", name),
            name.to_string(),
            "default".to_string(),
            "node1".to_string(),
            "pod1".to_string(),
            vec!["container1".to_string()],
            vec![1234],
            false,
            Arc::new(MockMetrics {
                name: name.to_string(),
            }),
            TestState {
                data: "test".to_string(),
            },
        )
    }

    #[tokio::test]
    async fn test_session_key_generation() {
        let key = TestManager::session_key("watcher", "ns", "pod");
        assert_eq!(key, "watcher:ns:pod");
    }

    #[tokio::test]
    async fn test_session_insert_and_get() {
        let manager = TestManager {
            sessions: new_session_map(),
            max: 10,
        };

        let session = create_test_session("test1");
        let key = session.id.clone();

        let result = manager.insert(key.clone(), session).await;
        assert!(result.is_ok());

        let retrieved = manager.get(&key).await;
        assert!(retrieved.is_some());

        let session_arc = retrieved.unwrap();
        let session = session_arc.lock().await;
        assert_eq!(session.name, "test1");
    }

    #[tokio::test]
    async fn test_session_duplicate_insert() {
        let manager = TestManager {
            sessions: new_session_map(),
            max: 10,
        };

        let session1 = create_test_session("test1");
        let key = session1.id.clone();

        manager.insert(key.clone(), session1).await.unwrap();

        let session2 = create_test_session("test1");
        let result = manager.insert(key.clone(), session2).await;

        assert!(matches!(result, Err(SessionError::AlreadyExists { .. })));
    }

    #[tokio::test]
    async fn test_session_max_reached() {
        let manager = TestManager {
            sessions: new_session_map(),
            max: 2,
        };

        let session1 = create_test_session("test1");
        manager.insert(session1.id.clone(), session1).await.unwrap();

        let session2 = create_test_session("test2");
        manager.insert(session2.id.clone(), session2).await.unwrap();

        let session3 = create_test_session("test3");
        let result = manager.insert(session3.id.clone(), session3).await;

        assert!(matches!(result, Err(SessionError::MaxSessionsReached { .. })));
    }

    #[tokio::test]
    async fn test_session_remove() {
        let manager = TestManager {
            sessions: new_session_map(),
            max: 10,
        };

        let session = create_test_session("test1");
        let key = session.id.clone();
        let running = session.running.clone();

        manager.insert(key.clone(), session).await.unwrap();
        assert!(running.load(Ordering::SeqCst));

        let removed = manager.remove(&key).await;
        assert!(removed.is_some());

        // Session should be signaled to stop
        assert!(!running.load(Ordering::SeqCst));

        // Should no longer exist
        assert!(!manager.exists(&key).await);
    }

    #[tokio::test]
    async fn test_session_count() {
        let manager = TestManager {
            sessions: new_session_map(),
            max: 10,
        };

        assert_eq!(manager.count().await, 0);

        let session1 = create_test_session("test1");
        manager.insert(session1.id.clone(), session1).await.unwrap();
        assert_eq!(manager.count().await, 1);

        let session2 = create_test_session("test2");
        manager.insert(session2.id.clone(), session2).await.unwrap();
        assert_eq!(manager.count().await, 2);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let session = create_test_session("test1");
        assert!(session.is_running());

        session.stop();
        assert!(!session.is_running());
    }
}
