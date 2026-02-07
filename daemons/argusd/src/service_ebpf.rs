// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # ArgusdService gRPC Implementation (eBPF Mode)
//!
//! This module implements the gRPC service using eBPF LSM hooks instead of inotify.
//! It provides full process attribution for every file event.
//!
//! ## Differences from Traditional Mode
//!
//! | Aspect | Traditional (inotify) | eBPF (this module) |
//! |--------|----------------------|---------------------|
//! | Event source | inotify | LSM hooks |
//! | Process info | None | Full (PID, UID, comm, container_id) |
//! | Filtering | Userspace | Kernel (WATCHED_PREFIXES map) |
//! | Kernel requirement | 2.6.13+ | 5.8+ with BTF |

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use panoptes_common::{
    detect_runtime,
    // eBPF loader
    ebpf::{EbpfError, EbpfEventReceiver, EbpfLoader},
    // gRPC streaming helpers
    filtered_broadcast_stream,
    new_session_map,
    runtime_for_container,
    stream_from_iter,
    ContainerRuntime,
    // Metrics
    DaemonMetrics,
    // Event broadcasting
    EventBroadcaster,
    MetricsAggregator,
    // Session management
    Session,
    SessionManager,
    SessionMap,
    SessionState,
    StreamFilter,
};
use prost_types::Timestamp;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

use crate::ebpf::EbpfFileEvent;
use crate::metrics::WatcherMetrics;
use crate::proto::*;

/// Path to eBPF bytecode (embedded at build time or loaded from filesystem)
const EBPF_BYTECODE_PATH: &str = "/usr/lib/argusd/argusd-ebpf";

/// LSM programs to attach for FIM events
const LSM_PROGRAMS: &[&str] = &[
    "inode_create", // File creation
    "inode_unlink", // File deletion
    "inode_rename", // File rename/move
    "file_open",    // File open (for write detection)
];

/// Convert SystemTime to protobuf Timestamp.
fn system_time_to_timestamp(time: SystemTime) -> Option<Timestamp> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(Timestamp {
        seconds: duration.as_secs() as i64,
        nanos: duration.subsec_nanos() as i32,
    })
}

/// Argus eBPF session state for file integrity monitoring.
pub struct EbpfWatchSessionState {
    /// Watch subjects configuration.
    pub subjects: Vec<WatchSubject>,
    /// Log format template.
    pub log_format: String,
    /// Number of watched path prefixes in eBPF map.
    pub watched_prefixes: AtomicU64,
    /// Typed metrics for tracking.
    pub typed_metrics: Arc<WatcherMetrics>,
    /// Whether eBPF filters are configured and active.
    pub watches_ready: bool,
    /// When watches became ready (eBPF filters configured).
    pub ready_at: Option<SystemTime>,
    /// Path prefixes being watched (for removal on DestroyWatch).
    pub active_prefixes: Vec<String>,
}

impl SessionState for EbpfWatchSessionState {
    type Config = WatchSubject;
    type Event = FileEvent;
}

// Note: Filterable impl for FileEvent is in service.rs (shared between modes)

/// Argusd gRPC service implementation (eBPF mode).
pub struct ArgusdServiceImpl {
    /// Node name.
    node_name: String,
    /// Cluster name for multi-cluster deployments.
    cluster_name: String,
    /// Maximum number of watches.
    max_watches: usize,
    /// Active watch sessions.
    sessions: SessionMap<EbpfWatchSessionState>,
    /// Global event broadcaster for multi-client streaming.
    broadcaster: EventBroadcaster<FileEvent>,
    /// Aggregate metrics collector.
    metrics: Arc<MetricsAggregator>,
    /// Detected container runtime.
    runtime: Option<Box<dyn ContainerRuntime>>,
    /// Shared eBPF loader (single instance for all sessions).
    ebpf_loader: Arc<Mutex<Option<EbpfLoader>>>,
    /// Event receiver from eBPF (single instance).
    ebpf_receiver: Arc<Mutex<Option<EbpfEventReceiver>>>,
}

/// Implement SessionManager trait for ArgusdServiceImpl.
impl SessionManager<EbpfWatchSessionState> for ArgusdServiceImpl {
    fn sessions(&self) -> &SessionMap<EbpfWatchSessionState> {
        &self.sessions
    }

    fn max_sessions(&self) -> usize {
        self.max_watches
    }
}

impl ArgusdServiceImpl {
    /// Create a new ArgusdService instance (eBPF mode).
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
            ebpf_loader: Arc::new(Mutex::new(None)),
            ebpf_receiver: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the eBPF subsystem (called on first CreateWatch).
    async fn ensure_ebpf_initialized(&self) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        if loader_guard.is_some() {
            return Ok(()); // Already initialized
        }

        info!(path = EBPF_BYTECODE_PATH, "Loading eBPF programs");

        let (mut loader, receiver) = EbpfLoader::load_from_path(EBPF_BYTECODE_PATH, LSM_PROGRAMS)?;

        // Enable kernel-side filtering
        loader.set_filter_enabled(true)?;

        // Attach process tracking tracepoints for exec-time caching
        // This enables full process attribution (exe, cmdline, cwd, ppid)
        if let Err(e) = loader.attach_process_tracepoints() {
            warn!(error = %e, "Failed to attach process tracepoints - process cache will be unavailable");
        } else {
            info!("Process cache tracepoints attached");
        }

        // Start the event loop
        loader.start_event_loop().await?;

        *loader_guard = Some(loader);
        drop(loader_guard);

        let mut receiver_guard = self.ebpf_receiver.lock().await;
        *receiver_guard = Some(receiver);
        drop(receiver_guard);

        // Start event forwarding task
        self.start_event_forwarder().await;

        info!("eBPF subsystem initialized");
        Ok(())
    }

    /// Start the background task that forwards eBPF events to the broadcaster.
    async fn start_event_forwarder(&self) {
        let receiver = self.ebpf_receiver.clone();
        let loader = self.ebpf_loader.clone();
        let broadcaster = self.broadcaster.clone();
        let sessions = self.sessions.clone();
        let node_name = self.node_name.clone();
        let cluster_name = self.cluster_name.clone();

        tokio::spawn(async move {
            loop {
                let event = {
                    let mut guard = receiver.lock().await;
                    if let Some(ref mut rx) = *guard {
                        rx.recv().await
                    } else {
                        None
                    }
                };

                let Some(raw_event) = event else {
                    warn!("eBPF event channel closed");
                    break;
                };

                // Convert to Argus event
                let ebpf_event = EbpfFileEvent::from(raw_event);

                // Look up cached process info (exe, cmdline, cwd, ppid)
                let cached_process = {
                    let loader_guard = loader.lock().await;
                    if let Some(ref ldr) = *loader_guard {
                        ldr.get_cached_process(raw_event.tgid).ok().flatten()
                    } else {
                        None
                    }
                };

                // Find matching session based on path prefix
                let sessions_guard = sessions.read().await;
                for session_arc in sessions_guard.values() {
                    let session = session_arc.lock().await;

                    // Check if this event matches any of the session's watched prefixes
                    let matches = session
                        .state
                        .active_prefixes
                        .iter()
                        .any(|prefix| ebpf_event.path.starts_with(prefix));

                    if !matches {
                        continue;
                    }

                    // Convert to proto event
                    let container_id = session
                        .container_ids
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    // Build ProcessInfo with cached data
                    let process_info = Some(crate::proto::ProcessInfo {
                        pid: raw_event.pid as i32,
                        tid: raw_event.tgid as i32,
                        uid: raw_event.uid as i32,
                        gid: raw_event.gid as i32,
                        comm: ebpf_event.comm.clone(),
                        // Use cached process info if available
                        exe: cached_process
                            .as_ref()
                            .map(|c| c.exe_str().to_string())
                            .unwrap_or_default(),
                        ppid: cached_process.as_ref().map(|c| c.ppid as i32).unwrap_or(0),
                        cmdline: cached_process
                            .as_ref()
                            .map(|c| {
                                c.cmdline_str()
                                    .split('\0')
                                    .filter(|s| !s.is_empty())
                                    .map(String::from)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        cwd: cached_process
                            .as_ref()
                            .map(|c| c.cwd_str().to_string())
                            .unwrap_or_default(),
                    });

                    let proto_event = crate::proto::FileEvent {
                        timestamp: Some(prost_types::Timestamp {
                            seconds: (ebpf_event.timestamp_ns / 1_000_000_000) as i64,
                            nanos: (ebpf_event.timestamp_ns % 1_000_000_000) as i32,
                        }),
                        watcher_name: session.name.clone(),
                        namespace: session.namespace.clone(),
                        node_name: node_name.clone(),
                        pod_name: session.pod_name.clone(),
                        container_id: container_id.to_string(),
                        event_type: ebpf_event.to_inotify_event() as i32,
                        path: ebpf_event.path.clone(),
                        filename: ebpf_event.path.rsplit('/').next().unwrap_or("").to_string(),
                        is_directory: false,
                        inode: 0,
                        tags: Default::default(),
                        process_info,
                        // Multi-cluster identification
                        cluster_name: cluster_name.clone(),
                    };

                    // Log the event with process attribution
                    let exe_info = cached_process
                        .as_ref()
                        .filter(|c| c.has_exe())
                        .map(|c| format!(" ({})", c.exe_str()))
                        .unwrap_or_default();
                    info!(
                        "<{}> file '{}' by {}:{}{} ({}:{})",
                        ebpf_event.event_type_str().to_uppercase(),
                        ebpf_event.path,
                        ebpf_event.comm,
                        ebpf_event.pid,
                        exe_info,
                        session.pod_name,
                        node_name
                    );

                    // Broadcast to all connected stream clients
                    let _ = broadcaster.send(proto_event);

                    // Update metrics
                    session
                        .state
                        .typed_metrics
                        .record_event_typed(ebpf_event.event_type_str());
                }
            }
        });
    }

    /// Add watched prefixes to the eBPF filter map.
    async fn add_watched_prefixes(&self, prefixes: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for prefix in prefixes {
            loader.add_watched_prefix(prefix, "WATCHED_PREFIXES")?;
        }

        Ok(())
    }

    /// Remove watched prefixes from the eBPF filter map.
    async fn remove_watched_prefixes(&self, prefixes: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for prefix in prefixes {
            loader.remove_watched_prefix(prefix, "WATCHED_PREFIXES")?;
        }

        Ok(())
    }

    /// Resolve container ID to PID using the detected runtime.
    #[allow(clippy::result_large_err)]
    fn resolve_container_pid(&self, container_id: &str) -> Result<i32, Status> {
        if let Ok(runtime) = runtime_for_container(container_id) {
            return runtime
                .resolve_pid(container_id)
                .map(|pid| pid as i32)
                .map_err(|e| Status::not_found(format!("Failed to resolve container PID: {}", e)));
        }

        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| Status::failed_precondition("No container runtime available"))?;

        runtime
            .resolve_pid(container_id)
            .map(|pid| pid as i32)
            .map_err(|e| Status::not_found(format!("Failed to resolve container PID: {}", e)))
    }

    /// Build list of path prefixes from watch subjects.
    #[allow(clippy::result_large_err)]
    fn build_prefixes(
        &self,
        subjects: &[WatchSubject],
        container_id: &str,
    ) -> Result<Vec<String>, Status> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| Status::failed_precondition("No container runtime"))?;

        let pid = self.resolve_container_pid(container_id)? as u32;
        let container_root = runtime.resolve_container_root(pid);

        let mut prefixes = Vec::new();

        for subject in subjects {
            for path in &subject.paths {
                let full_path = if let Some(stripped) = path.strip_prefix('/') {
                    container_root.join(stripped)
                } else {
                    container_root.join(path)
                };
                prefixes.push(full_path.to_string_lossy().to_string());
            }
        }

        Ok(prefixes)
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

#[tonic::async_trait]
impl argusd_service_server::ArgusdService for ArgusdServiceImpl {
    /// Create a new watch for monitoring files in containers (eBPF mode).
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
            "CreateWatch request (eBPF mode)"
        );

        let watch_id = Self::session_key(&req.watcher_name, &req.namespace, &req.pod_name);

        // Check if watch already exists
        if self.exists(&watch_id).await {
            return Err(Status::already_exists(format!(
                "Watch already exists: {}",
                watch_id
            )));
        }

        // Initialize eBPF subsystem if needed
        self.ensure_ebpf_initialized()
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize eBPF: {}", e)))?;

        // Resolve PIDs for all containers
        let mut pids = Vec::new();
        for container_id in &req.container_ids {
            match self.resolve_container_pid(container_id) {
                Ok(pid) => pids.push(pid),
                Err(e) => {
                    warn!(container = %container_id, error = %e, "Failed to resolve container PID");
                }
            }
        }

        if pids.is_empty() {
            pids = req.pids.clone();
        }

        // Build path prefixes from subjects
        let mut all_prefixes = Vec::new();
        for container_id in &req.container_ids {
            match self.build_prefixes(&req.subjects, container_id) {
                Ok(prefixes) => all_prefixes.extend(prefixes),
                Err(e) => {
                    warn!(container = %container_id, error = %e, "Failed to build prefixes");
                }
            }
        }

        // Create typed metrics for this session
        let typed_metrics = Arc::new(WatcherMetrics::new(&watch_id));
        let session_metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();
        self.metrics.register(session_metrics.clone()).await;

        // Create eBPF-specific state
        let watch_state = EbpfWatchSessionState {
            subjects: req.subjects.clone(),
            log_format: req.log_format.clone(),
            watched_prefixes: AtomicU64::new(all_prefixes.len() as u64),
            typed_metrics: typed_metrics.clone(),
            watches_ready: false,
            ready_at: None,
            active_prefixes: all_prefixes.clone(),
        };

        // Create session
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

        // Insert session
        let session_arc = self
            .insert(watch_id.clone(), session)
            .await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let mut watched_paths = 0;
        let mut watches_ready = false;

        // If not paused, configure eBPF filters
        if !req.paused && !all_prefixes.is_empty() {
            match self.add_watched_prefixes(&all_prefixes).await {
                Ok(()) => {
                    watched_paths = all_prefixes.len() as i32;
                    watches_ready = true;

                    let ready_time = SystemTime::now();
                    {
                        let mut session_guard = session_arc.lock().await;
                        session_guard.state.watches_ready = true;
                        session_guard.state.ready_at = Some(ready_time);
                    }

                    info!(
                        watch_id = %watch_id,
                        prefixes = all_prefixes.len(),
                        "eBPF filters configured"
                    );
                }
                Err(e) => {
                    warn!(error = %e, "Failed to configure eBPF filters");
                }
            }
        }

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
            "DestroyWatch request (eBPF mode)"
        );

        let watch_id = Self::session_key(&req.watcher_name, &req.namespace, &req.pod_name);

        // Remove session
        let session = self.remove(&watch_id).await;

        if let Some(session) = session {
            let session = session.lock().await;

            // Remove eBPF filters
            if !session.state.active_prefixes.is_empty() {
                if let Err(e) = self
                    .remove_watched_prefixes(&session.state.active_prefixes)
                    .await
                {
                    warn!(error = %e, "Failed to remove eBPF filters");
                }
            }

            // Unregister metrics
            self.metrics.unregister(&watch_id).await;

            info!(watch_id = %watch_id, "Watch destroyed");
        } else {
            warn!(watch_id = %watch_id, "Watch not found for destruction");
        }

        Ok(Response::new(()))
    }

    type GetWatchStateStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<WatchState, Status>> + Send>>;

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

        let sessions = self.sessions.read().await;
        let mut states = Vec::new();

        for session_arc in sessions.values() {
            let session = session_arc.lock().await;

            if !req.watcher_name.is_empty() && session.name != req.watcher_name {
                continue;
            }
            if !req.namespace.is_empty() && session.namespace != req.namespace {
                continue;
            }

            let state = WatchState {
                watcher_name: session.name.clone(),
                namespace: session.namespace.clone(),
                node_name: session.node_name.clone(),
                pod_name: session.pod_name.clone(),
                pids: session.pids.clone(),
                watch_descriptors: session.state.watched_prefixes.load(Ordering::Relaxed) as i32,
                created_at: system_time_to_timestamp(session.created_at),
                subjects: session.state.subjects.clone(),
                log_format: session.state.log_format.clone(),
                paused: session.paused,
                watches_ready: session.state.watches_ready,
                ready_at: session.state.ready_at.and_then(system_time_to_timestamp),
                active_watch_descriptors: session.state.watched_prefixes.load(Ordering::Relaxed)
                    as i32,
            };

            states.push(state);
        }

        Ok(stream_from_iter(states, 100))
    }

    type StreamEventsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<FileEvent, Status>> + Send>>;

    /// Stream real-time file events.
    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();

        info!(
            watcher = %req.watcher_name,
            namespace = %req.namespace,
            "StreamEvents started (eBPF mode)"
        );

        let mut filter = StreamFilter::new();
        if !req.watcher_name.is_empty() {
            filter = filter.with_name(req.watcher_name);
        }
        if !req.namespace.is_empty() {
            filter = filter.with_namespace(req.namespace);
        }
        if !req.event_types.is_empty() {
            let event_type_strings: Vec<String> = req
                .event_types
                .iter()
                .map(|e| inotify_event_to_string(*e))
                .filter(|s| !s.is_empty())
                .collect();
            filter = filter.with_event_types(event_type_strings);
        }

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
                let event_counts: HashMap<String, i64> = s
                    .custom
                    .iter()
                    .map(|(k, v)| (k.clone(), *v as i64))
                    .collect();

                WatchMetrics {
                    watcher_name: s.name.clone(),
                    namespace: String::new(),
                    event_counts,
                    queue_overflows: s.custom.get("queue_overflows").copied().unwrap_or(0) as i64,
                }
            })
            .collect();

        let total_watch_descriptors: i32 = snapshots
            .iter()
            .map(|s| s.custom.get("prefixes_active").copied().unwrap_or(0) as i32)
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
            "UpdateWatch request (eBPF mode)"
        );

        let session_arc = self
            .get(&key)
            .await
            .ok_or_else(|| Status::not_found(format!("Watch not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        match UpdateAction::try_from(req.action).unwrap_or(UpdateAction::Unspecified) {
            UpdateAction::Pause => {
                if session.paused {
                    return Err(Status::failed_precondition("Watch is already paused"));
                }

                // Remove prefixes from eBPF filter
                if !session.state.active_prefixes.is_empty() {
                    self.remove_watched_prefixes(&session.state.active_prefixes)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to pause: {}", e)))?;
                }

                session.paused = true;
                info!(watch_id = %key, "Watch paused");

                Ok(Response::new(UpdateWatchResponse {
                    watch_id: key,
                    paused: true,
                    watched_paths: 0,
                }))
            }
            UpdateAction::Resume => {
                if !session.paused {
                    return Err(Status::failed_precondition("Watch is not paused"));
                }

                // Re-add prefixes to eBPF filter
                if !session.state.active_prefixes.is_empty() {
                    self.add_watched_prefixes(&session.state.active_prefixes)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to resume: {}", e)))?;
                }

                session.paused = false;
                let watched_paths = session.state.active_prefixes.len() as i32;

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
        let id = ArgusdServiceImpl::session_key("my-watcher", "default", "my-pod");
        assert_eq!(id, "my-watcher:default:my-pod");
    }

    #[test]
    fn test_inotify_event_to_string() {
        assert_eq!(
            inotify_event_to_string(InotifyEvent::Create as i32),
            "create"
        );
        assert_eq!(
            inotify_event_to_string(InotifyEvent::Modify as i32),
            "modify"
        );
        assert_eq!(
            inotify_event_to_string(InotifyEvent::Delete as i32),
            "delete"
        );
    }

    #[tokio::test]
    async fn test_service_creation() {
        let service = ArgusdServiceImpl::new("test-node".to_string(), "".to_string(), 1000);
        assert_eq!(service.node_name, "test-node");
        assert_eq!(service.max_watches, 1000);
    }
}
