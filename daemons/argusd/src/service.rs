// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # ArgusdService gRPC Implementation
//!
//! This module implements the gRPC service defined in `argus.v2.proto`.
//!
//! ## RPCs Implemented
//!
//! - `CreateWatch` - Establish a new inotify watch for container file monitoring
//! - `DestroyWatch` - Remove an existing watch
//! - `GetWatchState` - Stream current state of all watches
//! - `StreamEvents` - Stream real-time file system events
//! - `GetMetrics` - Retrieve daemon metrics
//! - `UpdateWatch` - Pause or resume an existing watch

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use panoptes_common::{
    detect_runtime, runtime_for_container, ContainerRuntime,
    // Session management
    Session, SessionManager, SessionMap, SessionState, new_session_map,
    // Event broadcasting
    EventBroadcaster, Filterable, StreamFilter,
    // Metrics
    DaemonMetrics, MetricsAggregator,
    // gRPC streaming helpers
    filtered_broadcast_stream, stream_from_iter,
};
use prost_types::Timestamp;
use tokio::sync::{mpsc, Mutex};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::metrics::WatcherMetrics;
use crate::notify::{EventType, FileEvent as NotifyFileEvent, WatchConfig, Watcher};
use crate::proto::*;

/// Convert SystemTime to protobuf Timestamp.
fn system_time_to_timestamp(time: SystemTime) -> Option<Timestamp> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(Timestamp {
        seconds: duration.as_secs() as i64,
        nanos: duration.subsec_nanos() as i32,
    })
}

/// Argus-specific session state for file integrity monitoring.
///
/// This state is stored within the generic `Session<WatchSessionState>` container.
pub struct WatchSessionState {
    /// Watch subjects configuration.
    pub subjects: Vec<WatchSubject>,
    /// Log format template.
    pub log_format: String,
    /// Number of active watch descriptors.
    pub watch_descriptors: AtomicU64,
    /// Event channel sender for this session (for resume).
    pub event_tx: Option<mpsc::Sender<NotifyFileEvent>>,
    /// Watcher instance (shared for pause/resume via UpdateWatch).
    pub watcher: Option<Arc<Mutex<Watcher>>>,
    /// Typed metrics for inotify-specific tracking (Watcher needs this type).
    pub typed_metrics: Arc<WatcherMetrics>,
    /// Watch configuration (for resume).
    pub watch_config: Option<WatchConfig>,
    /// Whether all inotify watches have been registered and are active.
    /// Used by watcher-wait init container to block pod startup.
    pub watches_ready: bool,
    /// When watches became ready (inotify registration completed).
    pub ready_at: Option<SystemTime>,
}

impl SessionState for WatchSessionState {
    type Config = WatchSubject;
    type Event = FileEvent;
}

/// Implement Filterable for FileEvent to enable stream filtering.
impl Filterable for FileEvent {
    fn filter_name(&self) -> &str {
        &self.watcher_name
    }

    fn filter_namespace(&self) -> &str {
        &self.namespace
    }

    fn filter_event_type(&self) -> String {
        inotify_event_to_string(self.event_type)
    }
}

/// Argusd gRPC service implementation.
pub struct ArgusdServiceImpl {
    /// Node name.
    #[allow(dead_code)]
    node_name: String,
    /// Cluster name for multi-cluster deployments.
    cluster_name: String,
    /// Maximum number of watches per session.
    max_watches: usize,
    /// Active watch sessions (using generic session management).
    sessions: SessionMap<WatchSessionState>,
    /// Global event broadcaster for multi-client streaming.
    broadcaster: EventBroadcaster<FileEvent>,
    /// Aggregate metrics collector.
    metrics: Arc<MetricsAggregator>,
    /// Detected container runtime.
    runtime: Option<Box<dyn ContainerRuntime>>,
}

/// Implement SessionManager trait for ArgusdServiceImpl.
impl SessionManager<WatchSessionState> for ArgusdServiceImpl {
    fn sessions(&self) -> &SessionMap<WatchSessionState> {
        &self.sessions
    }

    fn max_sessions(&self) -> usize {
        self.max_watches
    }
}

impl ArgusdServiceImpl {
    /// Create a new ArgusdService instance.
    pub fn new(node_name: String, cluster_name: String, max_watches: usize) -> Self {
        let runtime: Option<Box<dyn ContainerRuntime>> = detect_runtime();

        if let Some(ref rt) = runtime {
            info!(runtime = ?rt.runtime_type(), "Detected container runtime");
        } else {
            warn!("No container runtime detected");
        }

        Self {
            node_name: node_name.clone(),
            cluster_name,
            max_watches,
            sessions: new_session_map(),
            broadcaster: EventBroadcaster::new(10000),
            metrics: Arc::new(MetricsAggregator::new(node_name)),
            runtime,
        }
    }

    /// Resolve container ID to PID using the detected runtime.
    fn resolve_container_pid(&self, container_id: &str) -> Result<i32, Status> {
        // Try to get runtime for this specific container first
        if let Ok(runtime) = runtime_for_container(container_id) {
            return runtime.resolve_pid(container_id)
                .map(|pid| pid as i32)
                .map_err(|e| Status::not_found(format!("Failed to resolve container PID: {}", e)));
        }

        // Fall back to detected runtime
        let runtime = self.runtime.as_ref()
            .ok_or_else(|| Status::failed_precondition("No container runtime available"))?;

        runtime.resolve_pid(container_id)
            .map(|pid| pid as i32)
            .map_err(|e| Status::not_found(format!("Failed to resolve container PID: {}", e)))
    }

    /// Build watch config from proto subjects.
    fn build_watch_config(
        &self,
        subjects: &[WatchSubject],
        container_id: &str,
    ) -> Result<WatchConfig, Status> {
        let runtime = self.runtime.as_ref()
            .ok_or_else(|| Status::failed_precondition("No container runtime"))?;

        let pid = self.resolve_container_pid(container_id)? as u32;
        let container_root = runtime.resolve_container_root(pid);

        let mut paths = Vec::new();
        let mut events = Vec::new();
        let mut ignore_patterns = Vec::new();
        let mut recursive = false;
        let mut max_depth = None;

        for subject in subjects {
            for path in &subject.paths {
                // Resolve path relative to container root
                let full_path = if path.starts_with('/') {
                    container_root.join(&path[1..])
                } else {
                    container_root.join(path)
                };
                paths.push(full_path);
            }

            for event in &subject.events {
                let event_str = inotify_event_to_string(*event);
                if !event_str.is_empty() {
                    events.push(event_str);
                }
            }

            ignore_patterns.extend(subject.ignore.iter().cloned());

            if subject.recursive {
                recursive = true;
            }

            if subject.max_depth > 0 {
                max_depth = Some(subject.max_depth as u32);
            }
        }

        if events.is_empty() {
            events.push("all".to_string());
        }

        Ok(WatchConfig {
            paths,
            events,
            ignore_patterns,
            recursive,
            max_depth,
        })
    }

}

/// Convert InotifyEvent enum to string.
fn inotify_event_to_string(event: i32) -> String {
    match InotifyEvent::try_from(event) {
        Ok(InotifyEvent::Access) => "access".to_string(),
        Ok(InotifyEvent::Attrib) => "attrib".to_string(),
        Ok(InotifyEvent::CloseWrite) => "closewrite".to_string(),
        Ok(InotifyEvent::CloseNowrite) => "closenowrite".to_string(),
        Ok(InotifyEvent::Create) => "create".to_string(),
        Ok(InotifyEvent::Delete) => "delete".to_string(),
        Ok(InotifyEvent::DeleteSelf) => "deleteself".to_string(),
        Ok(InotifyEvent::Modify) => "modify".to_string(),
        Ok(InotifyEvent::MoveSelf) => "moveself".to_string(),
        Ok(InotifyEvent::MovedFrom) => "movedfrom".to_string(),
        Ok(InotifyEvent::MovedTo) => "movedto".to_string(),
        Ok(InotifyEvent::Open) => "open".to_string(),
        Ok(InotifyEvent::All) => "all".to_string(),
        _ => String::new(),
    }
}

/// Convert internal EventType to proto InotifyEvent.
fn event_type_to_proto(event_type: &EventType) -> InotifyEvent {
    match event_type {
        EventType::Access => InotifyEvent::Access,
        EventType::Attrib => InotifyEvent::Attrib,
        EventType::CloseWrite => InotifyEvent::CloseWrite,
        EventType::CloseNoWrite => InotifyEvent::CloseNowrite,
        EventType::Create => InotifyEvent::Create,
        EventType::Delete => InotifyEvent::Delete,
        EventType::DeleteSelf => InotifyEvent::DeleteSelf,
        EventType::Modify => InotifyEvent::Modify,
        EventType::MoveSelf => InotifyEvent::MoveSelf,
        EventType::MovedFrom => InotifyEvent::MovedFrom,
        EventType::MovedTo => InotifyEvent::MovedTo,
        EventType::Open => InotifyEvent::Open,
        EventType::Unknown => InotifyEvent::Unspecified,
    }
}

#[tonic::async_trait]
impl argusd_service_server::ArgusdService for ArgusdServiceImpl {
    /// Create a new watch for monitoring files in containers.
    async fn create_watch(
        &self,
        request: Request<CreateWatchRequest>,
    ) -> Result<Response<CreateWatchResponse>, Status> {
        let req = request.into_inner();

        info!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            containers = ?req.container_ids,
            paused = req.paused,
            "CreateWatch request"
        );

        let watch_id = Self::session_key(&req.watcher_name, &req.namespace, &req.pod_name);

        // Check if watch already exists (using SessionManager trait method)
        if self.exists(&watch_id).await {
            return Err(Status::already_exists(format!(
                "Watch already exists: {}", watch_id
            )));
        }

        // Resolve PIDs for all containers
        let mut pids = Vec::new();
        for container_id in &req.container_ids {
            match self.resolve_container_pid(container_id) {
                Ok(pid) => pids.push(pid),
                Err(e) => {
                    warn!(container = %container_id, error = %e, "Failed to resolve container PID");
                    // Continue with other containers
                }
            }
        }

        // Use provided PIDs if container resolution failed
        if pids.is_empty() {
            pids = req.pids.clone();
        }

        // Create typed metrics for this session (implements DaemonMetrics trait)
        let typed_metrics = Arc::new(WatcherMetrics::new(&watch_id));
        // Cast to trait object for the common aggregator
        let session_metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();
        self.metrics.register(session_metrics.clone()).await;

        // Create Argus-specific state with typed metrics for Watcher
        let watch_state = WatchSessionState {
            subjects: req.subjects.clone(),
            log_format: req.log_format.clone(),
            watch_descriptors: AtomicU64::new(0),
            event_tx: None,
            watcher: None,
            typed_metrics: typed_metrics.clone(),
            watch_config: None,
            watches_ready: false,  // Will be set true after inotify registration
            ready_at: None,
        };

        // Create generic session with Argus-specific state
        let session = Session::new(
            watch_id.clone(),
            req.watcher_name.clone(),
            req.namespace.clone(),
            req.node_name.clone(),
            req.pod_name.clone(),
            req.container_ids.clone(),
            pids,
            req.paused,
            session_metrics,
            watch_state,
        );

        // Insert session using SessionManager trait method
        let session_arc = self.insert(watch_id.clone(), session).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let mut watched_paths = 0;
        let mut watches_ready = false;

        // If not paused, start watching SYNCHRONOUSLY
        // This ensures watches are registered before we return, so the
        // watcher-wait init container can rely on watches_ready=true
        if !req.paused {
            for container_id in &req.container_ids {
                match self.build_watch_config(&req.subjects, container_id) {
                    Ok(config) => {
                        watched_paths += config.paths.len() as i32;

                        // Create and start watcher with typed metrics
                        let session_guard = session_arc.lock().await;
                        let typed_metrics = session_guard.state.typed_metrics.clone();
                        drop(session_guard);

                        match Watcher::with_metrics(self.max_watches, typed_metrics) {
                            Ok(mut watcher) => {
                                let (tx, mut rx) = mpsc::channel::<NotifyFileEvent>(1000);

                                // SYNCHRONOUS: Register inotify watches BEFORE returning
                                // This is critical for the hardening pattern - ensures watches
                                // are active before the watcher-wait init container exits.
                                match watcher.add_watches(&config) {
                                    Ok(wd_count) => {
                                        info!(
                                            watch_id = %watch_id,
                                            paths = config.paths.len(),
                                            watch_descriptors = wd_count,
                                            "inotify watches registered SYNCHRONOUSLY"
                                        );

                                        // Mark watches as ready
                                        let ready_time = SystemTime::now();
                                        {
                                            let mut session_guard = session_arc.lock().await;
                                            session_guard.state.watches_ready = true;
                                            session_guard.state.ready_at = Some(ready_time);
                                            session_guard.state.watch_descriptors.store(wd_count as u64, Ordering::Relaxed);
                                        }
                                        watches_ready = true;
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "Failed to register inotify watches");
                                        // watches_ready remains false
                                    }
                                }

                                // Wrap watcher in Arc<Mutex> for shared access (UpdateWatch)
                                let watcher_arc = Arc::new(Mutex::new(watcher));

                                let broadcaster = self.broadcaster.clone();
                                let session_clone = session_arc.clone();
                                let container_id_clone = container_id.clone();
                                let node_name = req.node_name.clone();
                                let cluster_name = self.cluster_name.clone();

                                // Store watcher and event_tx in session for UpdateWatch
                                {
                                    let mut session_guard = session_arc.lock().await;
                                    session_guard.state.watcher = Some(watcher_arc.clone());
                                    session_guard.state.event_tx = Some(tx.clone());
                                    session_guard.state.watch_config = Some(config.clone());
                                }

                                // Spawn event forwarding task
                                tokio::spawn(async move {
                                    while let Some(event) = rx.recv().await {
                                        let session = session_clone.lock().await;
                                        let pod_name = session.pod_name.clone();
                                        let proto_event = FileEvent {
                                            timestamp: system_time_to_timestamp(SystemTime::now()),
                                            watcher_name: session.name.clone(),
                                            namespace: session.namespace.clone(),
                                            node_name: node_name.clone(),
                                            pod_name: pod_name.clone(),
                                            container_id: container_id_clone.clone(),
                                            event_type: event_type_to_proto(&event.event_type) as i32,
                                            path: event.path.to_string_lossy().to_string(),
                                            filename: event.filename.clone().unwrap_or_default(),
                                            is_directory: event.is_dir,
                                            inode: 0,
                                            tags: HashMap::new(),
                                            // Argus (inotify) doesn't provide process info - only Janus (fanotify) does
                                            process_info: None,
                                            // Multi-cluster identification
                                            cluster_name: cluster_name.clone(),
                                        };
                                        drop(session);

                                        // Log the file event
                                        info!(
                                            "<{}> {} '{}' ({}:{})",
                                            event.event_type.as_str().to_uppercase(),
                                            if event.is_dir { "directory" } else { "file" },
                                            event.path.display(),
                                            pod_name,
                                            node_name
                                        );

                                        // Broadcast to all connected stream clients
                                        let _ = broadcaster.send(proto_event);
                                    }
                                });

                                // Spawn watcher event loop task (watches already registered above)
                                let watcher_task = watcher_arc.clone();
                                tokio::spawn(async move {
                                    let mut watcher = watcher_task.lock().await;
                                    if let Err(e) = watcher.run_event_loop(tx).await {
                                        error!(error = %e, "Watcher event loop error");
                                    }
                                });
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to create watcher");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(container = %container_id, error = %e, "Failed to build watch config");
                    }
                }
            }
        }

        // Return watches_ready status so watcher-wait init container knows
        // that protection is active
        Ok(Response::new(CreateWatchResponse {
            watch_id,
            node_name: req.node_name.clone(),
            pod_name: req.pod_name,
            watched_paths,
            paused: req.paused,
            watches_ready,
        }))
    }

    /// Destroy an existing watch.
    async fn destroy_watch(
        &self,
        request: Request<DestroyWatchRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        info!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            "DestroyWatch request"
        );

        let watch_id = Self::session_key(&req.watcher_name, &req.namespace, &req.pod_name);

        // Remove session using SessionManager trait method (also signals stop)
        let session = self.remove(&watch_id).await;

        if let Some(session) = session {
            let session = session.lock().await;

            // Stop watcher if running
            if let Some(ref watcher_arc) = session.state.watcher {
                let watcher = watcher_arc.lock().await;
                watcher.stop();
            }

            // Unregister metrics
            self.metrics.unregister(&watch_id).await;

            info!(watch_id = %watch_id, "Watch destroyed");
        } else {
            warn!(watch_id = %watch_id, "Watch not found for destruction");
        }

        Ok(Response::new(()))
    }

    type GetWatchStateStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<WatchState, Status>> + Send>
    >;

    /// Get current state of all watches.
    async fn get_watch_state(
        &self,
        request: Request<GetWatchStateRequest>,
    ) -> Result<Response<Self::GetWatchStateStream>, Status> {
        let req = request.into_inner();

        debug!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            "GetWatchState request"
        );

        // Collect matching states
        let sessions = self.sessions.read().await;
        let mut states = Vec::new();

        for session_arc in sessions.values() {
            let session = session_arc.lock().await;

            // Apply filters
            if !req.watcher_name.is_empty() && session.name != req.watcher_name {
                continue;
            }
            if !req.namespace.is_empty() && session.namespace != req.namespace {
                continue;
            }

            // Build proto WatchState from session data
            let state = WatchState {
                watcher_name: session.name.clone(),
                namespace: session.namespace.clone(),
                node_name: session.node_name.clone(),
                pod_name: session.pod_name.clone(),
                pids: session.pids.clone(),
                watch_descriptors: session.state.watch_descriptors.load(Ordering::Relaxed) as i32,
                created_at: system_time_to_timestamp(session.created_at),
                subjects: session.state.subjects.clone(),
                log_format: session.state.log_format.clone(),
                paused: session.paused,
                // Readiness fields for watcher-wait init container
                watches_ready: session.state.watches_ready,
                ready_at: session.state.ready_at.and_then(system_time_to_timestamp),
                active_watch_descriptors: session.state.watch_descriptors.load(Ordering::Relaxed) as i32,
            };

            states.push(state);
        }

        // Use stream_from_iter helper
        Ok(stream_from_iter(states, 100))
    }

    type StreamEventsStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<FileEvent, Status>> + Send>
    >;

    /// Stream real-time file events.
    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();

        info!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            "StreamEvents started"
        );

        // Build filter from request
        let mut filter = StreamFilter::new();
        if !req.watcher_name.is_empty() {
            filter = filter.with_name(req.watcher_name);
        }
        if !req.namespace.is_empty() {
            filter = filter.with_namespace(req.namespace);
        }
        if !req.event_types.is_empty() {
            let event_type_strings: Vec<String> = req.event_types
                .iter()
                .map(|e| inotify_event_to_string(*e as i32))
                .filter(|s| !s.is_empty())
                .collect();
            filter = filter.with_event_types(event_type_strings);
        }

        // Subscribe to broadcaster and create filtered stream
        let rx = self.broadcaster.subscribe();
        Ok(filtered_broadcast_stream(rx, filter, |e| e, 1000))
    }

    /// Get daemon metrics.
    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let req = request.into_inner();

        debug!(watcher = %req.watcher_name, "GetMetrics request");

        let snapshots = self.metrics.collect_all().await;
        let totals = self.metrics.totals();

        let watch_metrics: Vec<WatchMetrics> = snapshots
            .iter()
            .filter(|s| req.watcher_name.is_empty() || s.name.contains(&req.watcher_name))
            .map(|s| {
                // Convert custom metrics to event counts
                let event_counts: HashMap<String, i64> = s.custom
                    .iter()
                    .map(|(k, v)| (k.clone(), *v as i64))
                    .collect();

                WatchMetrics {
                    watcher_name: s.name.clone(),
                    namespace: String::new(), // Would need to parse from name
                    event_counts,
                }
            })
            .collect();

        let total_watch_descriptors: i32 = snapshots
            .iter()
            .map(|s| s.custom.get("watches_active").copied().unwrap_or(0) as i32)
            .sum();

        Ok(Response::new(MetricsResponse {
            active_watches: totals.active_sessions as i32,
            total_watch_descriptors,
            events_processed: totals.events_total as i64,
            watch_metrics,
        }))
    }

    /// Update an existing watch (pause/resume).
    async fn update_watch(
        &self,
        request: Request<UpdateWatchRequest>,
    ) -> Result<Response<UpdateWatchResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.watcher_name, &req.namespace, &req.pod_name);

        info!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            action = ?req.action,
            "UpdateWatch request"
        );

        // Get session
        let session_arc = self.get(&key).await
            .ok_or_else(|| Status::not_found(format!("Watch not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        match UpdateAction::try_from(req.action).unwrap_or(UpdateAction::Unspecified) {
            UpdateAction::Pause => {
                if session.paused {
                    return Err(Status::failed_precondition("Watch is already paused"));
                }

                // Pause the watcher
                if let Some(ref watcher_arc) = session.state.watcher {
                    let mut watcher = watcher_arc.lock().await;
                    if let Err(e) = watcher.pause() {
                        return Err(Status::internal(format!("Failed to pause watcher: {}", e)));
                    }
                }

                session.paused = true;
                info!(watch_id = %key, "Watch paused");

                Ok(Response::new(UpdateWatchResponse {
                    watch_id: key,
                    paused: true,
                    watched_paths: 0, // No active watches when paused
                }))
            }
            UpdateAction::Resume => {
                if !session.paused {
                    return Err(Status::failed_precondition("Watch is not paused"));
                }

                // Resume the watcher
                if let Some(ref watcher_arc) = session.state.watcher {
                    if let Some(ref tx) = session.state.event_tx {
                        let mut watcher = watcher_arc.lock().await;
                        if let Err(e) = watcher.resume(tx.clone()).await {
                            return Err(Status::internal(format!("Failed to resume watcher: {}", e)));
                        }
                    } else {
                        return Err(Status::internal("No event channel available for resume"));
                    }
                } else {
                    return Err(Status::internal("No watcher available for resume"));
                }

                session.paused = false;
                let watched_paths = session.state.watch_config
                    .as_ref()
                    .map(|c| c.paths.len() as i32)
                    .unwrap_or(0);

                info!(watch_id = %key, "Watch resumed");

                Ok(Response::new(UpdateWatchResponse {
                    watch_id: key,
                    paused: false,
                    watched_paths,
                }))
            }
            UpdateAction::Unspecified => {
                Err(Status::invalid_argument("Action must be PAUSE or RESUME"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key_generation() {
        // Uses SessionManager trait method
        let id = ArgusdServiceImpl::session_key("my-watcher", "default", "my-pod");
        assert_eq!(id, "my-watcher:default:my-pod");
    }

    #[test]
    fn test_inotify_event_to_string() {
        assert_eq!(inotify_event_to_string(InotifyEvent::Create as i32), "create");
        assert_eq!(inotify_event_to_string(InotifyEvent::Modify as i32), "modify");
        assert_eq!(inotify_event_to_string(InotifyEvent::Delete as i32), "delete");
        assert_eq!(inotify_event_to_string(InotifyEvent::All as i32), "all");
    }

    #[test]
    fn test_event_type_to_proto() {
        assert_eq!(event_type_to_proto(&EventType::Create), InotifyEvent::Create);
        assert_eq!(event_type_to_proto(&EventType::Modify), InotifyEvent::Modify);
        assert_eq!(event_type_to_proto(&EventType::Delete), InotifyEvent::Delete);
        assert_eq!(event_type_to_proto(&EventType::Unknown), InotifyEvent::Unspecified);
    }

    #[test]
    fn test_system_time_to_timestamp() {
        let now = SystemTime::now();
        let ts = system_time_to_timestamp(now);
        assert!(ts.is_some());

        let ts = ts.unwrap();
        assert!(ts.seconds > 0);
    }

    #[tokio::test]
    async fn test_service_creation() {
        let service = ArgusdServiceImpl::new("test-node".to_string(), "test-cluster".to_string(), 1000);
        assert_eq!(service.node_name, "test-node");
        assert_eq!(service.cluster_name, "test-cluster");
        assert_eq!(service.max_watches, 1000);
    }

    #[test]
    fn test_file_event_filterable() {
        let event = FileEvent {
            timestamp: None,
            watcher_name: "test-watcher".to_string(),
            namespace: "default".to_string(),
            node_name: "node1".to_string(),
            pod_name: "pod1".to_string(),
            container_id: "container1".to_string(),
            event_type: InotifyEvent::Modify as i32,
            path: "/etc/hosts".to_string(),
            filename: "hosts".to_string(),
            is_directory: false,
            inode: 0,
            tags: HashMap::new(),
            process_info: None, // Argus (inotify) doesn't provide process info
            cluster_name: "test-cluster".to_string(),
        };

        assert_eq!(event.filter_name(), "test-watcher");
        assert_eq!(event.filter_namespace(), "default");
        assert_eq!(event.filter_event_type(), "modify");
    }
}
