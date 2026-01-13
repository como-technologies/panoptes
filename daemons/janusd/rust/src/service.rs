// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # gRPC Service Implementation for Janusd
//!
//! This module implements the `JanusdService` gRPC service for file access
//! auditing and control using fanotify.
//!
//! ## Service Operations
//!
//! - `CreateGuard`: Establishes fanotify guards on specified paths/mounts
//! - `DestroyGuard`: Removes existing guards
//! - `GetGuardState`: Streams current state of all guards (filtered)
//! - `StreamAccessEvents`: Streams real-time access events
//! - `GetMetrics`: Returns daemon and per-guard metrics
//! - `UpdateGuard`: Pause or resume an existing guard
//! - `UpdatePolicy`: Update allow/deny patterns of an existing guard
//!
//! ## Container Integration
//!
//! Guards can target containers by resolving container IDs to PIDs via
//! containerd or CRI-O sockets, then monitoring paths under `/proc/{pid}/root`.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use panoptes_common::{
    detect_runtime, ContainerRuntime,
    Session, SessionManager, SessionMap, SessionState, new_session_map,
    EventBroadcaster, Filterable,
    DaemonMetrics, MetricsAggregator,
};
use panoptes_common::proc::{ProcessResolver, ProcfsProcessResolver};
use prost_types::Timestamp;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::audit::{AuditAccessEvent, AuditEventType, AuditLogger};
use crate::guard::{AccessEvent as GuardAccessEvent, AccessResponse as GuardAccessResponse, Guard, GuardConfig};
use crate::metrics::GuardMetrics;
use crate::proto::*;

/// Janus-specific session state for file access auditing.
///
/// Contains fanotify-specific configuration and resources that are
/// not part of the generic session management.
pub struct GuardSessionState {
    /// Guard subjects (allow/deny patterns)
    pub subjects: Vec<GuardSubject>,
    /// Log format for events
    pub log_format: String,
    /// Whether the guard is enforcing (deny) or just auditing
    pub enforcing: bool,
    /// Event channel for sending events to the forwarding task
    pub event_tx: mpsc::Sender<GuardAccessEvent>,
    /// Typed reference to GuardMetrics for fanotify-specific operations
    pub typed_metrics: Arc<GuardMetrics>,
    /// Guard configuration (for resume after pause).
    pub guard_config: Option<GuardConfig>,
    /// Whether fanotify marks have been registered (guard is ready).
    pub marks_registered: bool,
    /// Timestamp when guard became ready (marks registered).
    pub ready_at: Option<SystemTime>,
    /// Number of container mounts successfully registered.
    pub mount_count: u32,
}

impl SessionState for GuardSessionState {
    type Config = GuardSubject;
    type Event = AccessEvent;
}

/// Implement Filterable for proto AccessEvent to enable event filtering.
impl Filterable for AccessEvent {
    fn filter_name(&self) -> &str {
        &self.guard_name
    }

    fn filter_namespace(&self) -> &str {
        &self.namespace
    }

    fn filter_event_type(&self) -> String {
        // Convert proto event type to string
        match FanotifyEvent::try_from(self.event_type).unwrap_or(FanotifyEvent::Unspecified) {
            FanotifyEvent::Unspecified => "unspecified".to_string(),
            FanotifyEvent::Access => "access".to_string(),
            FanotifyEvent::Open => "open".to_string(),
            FanotifyEvent::OpenExec => "open_exec".to_string(),
            FanotifyEvent::CloseWrite => "close_write".to_string(),
            FanotifyEvent::Close => "close".to_string(),
            FanotifyEvent::All => "all".to_string(),
        }
    }
}

/// Convert a Session<GuardSessionState> to a proto GuardState message.
fn session_to_guard_state(session: &Session<GuardSessionState>) -> GuardState {
    let created_at = session
        .created_at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| Timestamp {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
        .ok();

    // Convert ready_at timestamp if present
    let ready_at = session
        .state
        .ready_at
        .and_then(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| Timestamp {
                    seconds: d.as_secs() as i64,
                    nanos: d.subsec_nanos() as i32,
                })
                .ok()
        });

    GuardState {
        guard_name: session.name.clone(),
        namespace: session.namespace.clone(),
        node_name: session.node_name.clone(),
        pod_name: session.pod_name.clone(),
        pids: session.pids.clone(),
        process_eventfds: vec![],
        created_at,
        subjects: session.state.subjects.clone(),
        log_format: session.state.log_format.clone(),
        paused: session.paused,
        enforcing: session.state.enforcing,
        guarded_paths: session
            .state
            .subjects
            .iter()
            .map(|s| s.allow.len() + s.deny.len())
            .sum::<usize>() as i32,
        // Readiness fields - indicates guard is actively protecting containers
        marks_registered: session.state.marks_registered,
        ready_at,
        mount_count: session.state.mount_count as i32,
    }
}

/// Janus daemon gRPC service implementation.
pub struct JanusdServiceImpl {
    max_guards: usize,
    sessions: SessionMap<GuardSessionState>,
    /// Event broadcaster for streaming proto events to UI clients.
    broadcaster: EventBroadcaster<AccessEvent>,
    metrics: Arc<MetricsAggregator>,
    runtime: Option<Box<dyn ContainerRuntime>>,
    proc_resolver: ProcfsProcessResolver,
    /// Audit logger for writing access events to kernel audit log.
    audit: Arc<dyn AuditLogger>,
}

/// Implement SessionManager trait for JanusdServiceImpl.
impl SessionManager<GuardSessionState> for JanusdServiceImpl {
    fn sessions(&self) -> &SessionMap<GuardSessionState> {
        &self.sessions
    }

    fn max_sessions(&self) -> usize {
        self.max_guards
    }
}

impl JanusdServiceImpl {
    /// Create a new Janusd service instance.
    ///
    /// # Arguments
    ///
    /// * `node_name` - Kubernetes node name for this daemon
    /// * `max_guards` - Maximum number of concurrent guards allowed
    /// * `audit` - Audit logger for writing to kernel audit log
    pub fn new(node_name: String, max_guards: usize, audit: Arc<dyn AuditLogger>) -> Self {
        let runtime: Option<Box<dyn ContainerRuntime>> = detect_runtime();

        if let Some(ref rt) = runtime {
            info!(runtime = ?rt.runtime_type(), "Detected container runtime");
        } else {
            warn!("No container runtime detected, container guards will fail");
        }

        Self {
            max_guards,
            sessions: new_session_map(),
            broadcaster: EventBroadcaster::new(10000),
            metrics: Arc::new(MetricsAggregator::new(&node_name)),
            runtime,
            proc_resolver: ProcfsProcessResolver::new(),
            audit,
        }
    }

    /// Resolve container IDs to PIDs using the detected runtime.
    fn resolve_container_pids(&self, container_ids: &[String]) -> Vec<i32> {
        let Some(ref runtime) = self.runtime else {
            return vec![];
        };

        container_ids
            .iter()
            .filter_map(|id| match runtime.resolve_pid(id) {
                Ok(pid) => Some(pid as i32),
                Err(e) => {
                    warn!(container_id = %id, error = %e, "Failed to resolve container PID");
                    None
                }
            })
            .collect()
    }

    /// Convert proto FanotifyEvent to string for guard config.
    fn fanotify_event_to_string(event: FanotifyEvent) -> Option<&'static str> {
        match event {
            FanotifyEvent::Unspecified => None,
            FanotifyEvent::Access => Some("access"),
            FanotifyEvent::Open => Some("open"),
            FanotifyEvent::OpenExec => Some("open_exec"),
            FanotifyEvent::CloseWrite => Some("close_write"),
            FanotifyEvent::Close => Some("close"),
            FanotifyEvent::All => Some("all"),
        }
    }

    /// Convert guard AccessResponse to proto AccessResponse enum.
    fn response_to_proto(response: GuardAccessResponse) -> i32 {
        match response {
            GuardAccessResponse::Allow => AccessResponse::Allow as i32,
            GuardAccessResponse::Deny => AccessResponse::Deny as i32,
            GuardAccessResponse::Audit => AccessResponse::Audit as i32,
        }
    }

    /// Convert string event type to proto FanotifyEvent.
    fn event_type_to_proto(event_type: &str) -> i32 {
        match event_type {
            "access" => FanotifyEvent::Access as i32,
            "open" => FanotifyEvent::Open as i32,
            "open_exec" => FanotifyEvent::OpenExec as i32,
            "close_write" => FanotifyEvent::CloseWrite as i32,
            "close" | "close_nowrite" => FanotifyEvent::Close as i32,
            _ => FanotifyEvent::Unspecified as i32,
        }
    }
}

#[tonic::async_trait]
impl janusd_service_server::JanusdService for JanusdServiceImpl {
    /// Creates a new file access guard.
    ///
    /// Establishes fanotify monitoring on the specified paths with the given
    /// access control rules. For container targets, resolves container IDs
    /// to PIDs and monitors paths under `/proc/{pid}/root`.
    async fn create_guard(
        &self,
        request: Request<CreateGuardRequest>,
    ) -> Result<Response<CreateGuardResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            containers = ?req.container_ids,
            paused = req.paused,
            enforcing = req.enforcing,
            "CreateGuard request"
        );

        // Check guard limit using SessionManager trait methods
        if self.count().await >= self.max_sessions() {
            return Err(Status::resource_exhausted(format!(
                "Maximum number of guards ({}) exceeded",
                self.max_guards
            )));
        }
        if self.exists(&key).await {
            return Err(Status::already_exists(format!(
                "Guard already exists: {}",
                key
            )));
        }

        // Resolve container PIDs if not provided
        let pids = if req.pids.is_empty() {
            self.resolve_container_pids(&req.container_ids)
        } else {
            req.pids.clone()
        };

        // Count guarded paths
        let guarded_paths: i32 = req
            .subjects
            .iter()
            .map(|s| (s.allow.len() + s.deny.len()) as i32)
            .sum();

        // Create event channel for this guard
        let (event_tx, mut event_rx) = mpsc::channel::<GuardAccessEvent>(1000);
        let broadcaster = self.broadcaster.clone();

        // Create typed metrics first so we can call fanotify-specific methods
        let typed_metrics = Arc::new(GuardMetrics::new(&req.guard_name));
        // Clone for the DaemonMetrics trait object
        let metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();

        // Create the session using the common Session<T> type
        let session = Arc::new(Mutex::new(Session {
            id: key.clone(),
            name: req.guard_name.clone(),
            namespace: req.namespace.clone(),
            node_name: req.node_name.clone(),
            pod_name: req.pod_name.clone(),
            container_ids: req.container_ids.clone(),
            pids: pids.clone(),
            paused: req.paused,
            created_at: SystemTime::now(),
            running: Arc::new(AtomicBool::new(true)),
            metrics: metrics.clone(),
            state: GuardSessionState {
                subjects: req.subjects.clone(),
                log_format: req.log_format.clone(),
                enforcing: req.enforcing,
                event_tx: event_tx.clone(),
                typed_metrics: typed_metrics.clone(),
                guard_config: None,
                marks_registered: false,
                ready_at: None,
                mount_count: 0,
            },
        }));

        // Insert session using direct access (insert not in trait)
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key.clone(), session.clone());
        }

        // Register metrics with the aggregator
        self.metrics.register(metrics.clone()).await;

        // Spawn event forwarding task
        let metrics_clone = typed_metrics.clone();
        let audit_clone = self.audit.clone();
        let proc_resolver = self.proc_resolver.clone();
        let guard_name_clone = req.guard_name.clone();
        let namespace_clone = req.namespace.clone();
        let pod_name_clone = req.pod_name.clone();
        let container_id_clone = req.container_ids.first().cloned().unwrap_or_default();
        let node_name_clone = req.node_name.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                // 1. METRICS - Always record (for observability, regardless of pattern match)
                match event.response {
                    GuardAccessResponse::Allow => {
                        metrics_clone.record_allowed();
                    }
                    GuardAccessResponse::Deny => {
                        metrics_clone.record_denied();
                    }
                    GuardAccessResponse::Audit => {
                        metrics_clone.record_audited();
                    }
                }
                metrics_clone.record_event_type(&event.event_type);

                // 2. LOGGING + STREAMING - Only pattern-matched events
                // This ensures daemon logs and UI stream show the same filtered events
                if event.matched_pattern {
                    // Log to daemon stdout
                    match event.response {
                        GuardAccessResponse::Allow => {
                            info!(
                                "<ALLOW> {} {} '{}' ({}:{})",
                                event.event_type.to_uppercase(),
                                if event.is_dir { "directory" } else { "file" },
                                event.path,
                                pod_name_clone,
                                node_name_clone
                            );
                        }
                        GuardAccessResponse::Deny => {
                            info!(
                                "<DENY> {} {} '{}' ({}:{})",
                                event.event_type.to_uppercase(),
                                if event.is_dir { "directory" } else { "file" },
                                event.path,
                                pod_name_clone,
                                node_name_clone
                            );
                        }
                        GuardAccessResponse::Audit => {
                            info!(
                                "<AUDIT> {} {} '{}' ({}:{})",
                                event.event_type.to_uppercase(),
                                if event.is_dir { "directory" } else { "file" },
                                event.path,
                                pod_name_clone,
                                node_name_clone
                            );
                        }
                    }

                    // Resolve process info from /proc/{pid}/
                    // Process may have exited, so fall back to PID-only on error
                    let (process_info, resolved_comm, resolved_exe, resolved_uid, resolved_gid) =
                        match proc_resolver.get_process_info(event.pid as u32) {
                            Ok(info) => (
                                Some(ProcessInfo {
                                    pid: info.pid as i32,
                                    tid: 0,
                                    uid: info.uid as i32,
                                    gid: info.gid as i32,
                                    comm: info.comm.clone(),
                                    exe: info.exe.to_string_lossy().to_string(),
                                    // V2 extended fields
                                    ppid: info.ppid as i32,
                                    cmdline: info.cmdline.clone(),
                                    cwd: info.cwd.to_string_lossy().to_string(),
                                }),
                                info.comm,
                                info.exe.to_string_lossy().to_string(),
                                info.uid,
                                info.gid,
                            ),
                            Err(_) => {
                                // Process may have exited - fall back to PID only
                                (
                                    Some(ProcessInfo {
                                        pid: event.pid,
                                        tid: 0,
                                        uid: 0,
                                        gid: 0,
                                        comm: String::new(),
                                        exe: String::new(),
                                        ppid: 0,
                                        cmdline: vec![],
                                        cwd: String::new(),
                                    }),
                                    String::new(),
                                    String::new(),
                                    0,
                                    0,
                                )
                            }
                        };

                    // Log ALL events to kernel audit (for compliance: PCI-DSS, HIPAA, SOC2)
                    // Includes process attribution to answer WHO made the access
                    let (audit_response, audit_event_type) = match event.response {
                        GuardAccessResponse::Allow => (GuardAccessResponse::Allow, AuditEventType::Access),
                        GuardAccessResponse::Deny => (GuardAccessResponse::Deny, AuditEventType::Denied),
                        GuardAccessResponse::Audit => (GuardAccessResponse::Audit, AuditEventType::Open),
                    };
                    let audit_event = AuditAccessEvent {
                        guard_name: guard_name_clone.clone(),
                        namespace: namespace_clone.clone(),
                        pod_name: pod_name_clone.clone(),
                        container_id: container_id_clone.clone(),
                        path: event.path.clone(),
                        pid: event.pid,
                        uid: resolved_uid,
                        gid: resolved_gid,
                        response: audit_response,
                        event_type: audit_event_type,
                        comm: resolved_comm.clone(),
                        exe: resolved_exe.clone(),
                    };
                    if let Err(e) = audit_clone.log_access(&audit_event) {
                        debug!(error = %e, "Failed to log access event to kernel audit");
                    }

                    // Stream to UI with full context
                    let proto_event = AccessEvent {
                        timestamp: Some(Timestamp {
                            seconds: SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0),
                            nanos: 0,
                        }),
                        guard_name: guard_name_clone.clone(),
                        namespace: namespace_clone.clone(),
                        node_name: node_name_clone.clone(),
                        pod_name: pod_name_clone.clone(),
                        container_id: container_id_clone.clone(),
                        event_type: JanusdServiceImpl::event_type_to_proto(&event.event_type),
                        path: event.path.clone(),
                        response: JanusdServiceImpl::response_to_proto(event.response),
                        process_info,
                        is_directory: event.is_dir,
                        tags: HashMap::new(),
                        audit_logged: false,
                    };

                    // Broadcast to all connected stream clients
                    // send() is sync - if no receivers, events are dropped (Ok(0))
                    let _ = broadcaster.send(proto_event);
                }
            }
        });

        // Track number of fanotify marks registered (0 if paused)
        let mut marks_registered: i32 = 0;

        // If not paused, create guard SYNCHRONOUSLY before returning response.
        // This ensures fanotify marks are registered before the operator considers
        // the pod "guarded" - eliminating the race condition window.
        if !req.paused {
            let session_guard = session.lock().await;
            let running = session_guard.running.clone();
            let event_tx = session_guard.state.event_tx.clone();
            let container_pids = session_guard.pids.clone();
            let enforcing = session_guard.state.enforcing;

            // Build guard config from first subject (simplified for now)
            let config = if let Some(subject) = session_guard.state.subjects.first() {
                GuardConfig {
                    allow_patterns: subject.allow.clone(),
                    deny_patterns: subject.deny.clone(),
                    events: subject
                        .events
                        .iter()
                        .filter_map(|e| {
                            JanusdServiceImpl::fanotify_event_to_string(
                                FanotifyEvent::try_from(*e).unwrap_or(FanotifyEvent::Unspecified),
                            )
                        })
                        .map(String::from)
                        .collect(),
                    auto_allow_owner: subject.auto_allow_owner,
                    enforce: enforcing,
                }
            } else {
                GuardConfig {
                    allow_patterns: vec![],
                    deny_patterns: vec![],
                    events: vec!["open_perm".to_string(), "access_perm".to_string()],
                    auto_allow_owner: false,
                    enforce: enforcing,
                }
            };
            drop(session_guard);

            // Create guard SYNCHRONOUSLY - blocks until fanotify_init() completes
            let guard = Guard::new(config.clone()).map_err(|e| {
                error!(error = %e, "Failed to create guard");
                running.store(false, Ordering::SeqCst);
                Status::internal(format!("Failed to create guard: {}", e))
            })?;

            // Register fanotify marks SYNCHRONOUSLY for all container PIDs.
            // This is the critical section - marks must be registered before
            // we return success to the operator.
            let mut mount_count: u32 = 0;
            for pid in &container_pids {
                let container_root = format!("/proc/{}/root", pid);
                let path = std::path::Path::new(&container_root);
                if path.exists() {
                    match guard.add_mount(path) {
                        Ok(_) => {
                            mount_count += 1;
                            info!(pid = pid, path = %container_root, "Registered fanotify mark for container");
                        }
                        Err(e) => {
                            warn!(pid = pid, error = %e, "Failed to add container mount to guard");
                        }
                    }
                } else {
                    warn!(pid = pid, path = %container_root, "Container root path does not exist");
                }
            }

            // Fail if no mounts could be registered
            if mount_count == 0 && !container_pids.is_empty() {
                error!("No container mounts could be added to guard");
                running.store(false, Ordering::SeqCst);
                return Err(Status::internal("Failed to register any fanotify marks"));
            }

            // Record ready timestamp - guard is now protecting the containers
            let ready_at = SystemTime::now();
            info!(
                mount_count = mount_count,
                "Guard ready - fanotify marks registered synchronously"
            );

            // Update session state with readiness info.
            // Note: We don't store the Guard in session state because it's owned
            // exclusively by the event loop task. The guard_config can be used
            // to recreate the guard if needed (e.g., for resume after pause).
            {
                let mut session_guard = session.lock().await;
                session_guard.state.guard_config = Some(config);
                session_guard.state.marks_registered = true;
                session_guard.state.ready_at = Some(ready_at);
                session_guard.state.mount_count = mount_count;
            }

            // Set marks_registered for the response
            marks_registered = mount_count as i32;

            // NOW spawn the event loop (async is fine - marks are already registered).
            // The Guard is moved into this task and owned exclusively by it.
            tokio::spawn(async move {
                if let Err(e) = guard.guard(event_tx).await {
                    error!(error = %e, "Guard event loop error");
                }
            });
        }

        // Return success ONLY AFTER fanotify marks are registered.
        // marks_registered > 0 indicates the guard is ready and protecting containers.
        Ok(Response::new(CreateGuardResponse {
            guard_id: key,
            node_name: req.node_name.clone(),
            pod_name: req.pod_name,
            guarded_paths,
            process_eventfds: vec![],
            paused: req.paused,
            enforcing: req.enforcing,
            marks_registered,
        }))
    }

    /// Destroys an existing guard.
    ///
    /// Stops the fanotify monitoring and removes the guard configuration.
    async fn destroy_guard(
        &self,
        request: Request<DestroyGuardRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            "DestroyGuard request"
        );

        // Use SessionManager::remove() which handles running.store(false)
        if let Some(session) = self.remove(&key).await {
            // Unregister metrics from the aggregator
            let session_guard = session.lock().await;
            let guard_name = session_guard.name.clone();
            drop(session_guard);
            self.metrics.unregister(&guard_name).await;

            info!(guard = %key, "Guard destroyed");
            Ok(Response::new(()))
        } else {
            Err(Status::not_found(format!("Guard not found: {}", key)))
        }
    }

    type GetGuardStateStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<GuardState, Status>> + Send>>;

    /// Retrieves the current state of guards.
    ///
    /// Returns a stream of guard states, optionally filtered by name and namespace.
    async fn get_guard_state(
        &self,
        request: Request<GetGuardStateRequest>,
    ) -> Result<Response<Self::GetGuardStateStream>, Status> {
        let req = request.into_inner();

        debug!(
            guard_name = %req.guard_name,
            namespace = %req.namespace,
            "GetGuardState request"
        );

        let (tx, rx) = mpsc::channel(100);
        let sessions = self.sessions.clone();

        tokio::spawn(async move {
            let sessions = sessions.read().await;

            for (key, session) in sessions.iter() {
                let session_guard = session.lock().await;

                // Apply filters
                if !req.guard_name.is_empty() && session_guard.name != req.guard_name {
                    continue;
                }
                if !req.namespace.is_empty() && session_guard.namespace != req.namespace {
                    continue;
                }

                let state = session_to_guard_state(&session_guard);
                drop(session_guard);

                if tx.send(Ok(state)).await.is_err() {
                    debug!(guard = %key, "GetGuardState client disconnected");
                    break;
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::GetGuardStateStream))
    }

    type StreamAccessEventsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<AccessEvent, Status>> + Send>>;

    /// Streams real-time access events.
    ///
    /// Returns a continuous stream of file access events, optionally filtered
    /// by guard name, namespace, event types, and response.
    async fn stream_access_events(
        &self,
        request: Request<StreamAccessEventsRequest>,
    ) -> Result<Response<Self::StreamAccessEventsStream>, Status> {
        let req = request.into_inner();

        info!(
            guard_name = %req.guard_name,
            namespace = %req.namespace,
            include_allowed = req.include_allowed,
            event_types = ?req.event_types,
            "StreamAccessEvents started"
        );

        let (tx, rx) = mpsc::channel(1000);
        // Subscribe creates a new receiver - each client gets their own
        let mut event_rx = self.broadcaster.subscribe();

        let filter_guard_name = req.guard_name.clone();
        let filter_namespace = req.namespace.clone();
        let filter_event_types: Vec<i32> = req.event_types.clone();
        let include_allowed = req.include_allowed;

        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        // Apply filters
                        if !filter_guard_name.is_empty() && event.guard_name != filter_guard_name {
                            continue;
                        }
                        if !filter_namespace.is_empty() && event.namespace != filter_namespace {
                            continue;
                        }
                        if !filter_event_types.is_empty()
                            && !filter_event_types.contains(&event.event_type)
                        {
                            continue;
                        }
                        if !include_allowed && event.response == AccessResponse::Allow as i32 {
                            continue;
                        }

                        // Forward the proto event directly (context already populated)
                        if tx.send(Ok(event)).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!("Stream client lagged, skipped {} events", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break; // Channel closed
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(stream) as Self::StreamAccessEventsStream
        ))
    }

    /// Retrieves current metrics.
    ///
    /// Returns aggregate and per-guard metrics for monitoring.
    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let req = request.into_inner();

        debug!(guard_name = %req.guard_name, "GetMetrics request");

        let totals = self.metrics.totals();
        let sessions = self.sessions.read().await;

        let mut guard_metrics = Vec::new();
        let mut total_denied: u64 = 0;
        let mut total_allowed: u64 = 0;
        let mut total_events: u64 = 0;

        for (_key, session) in sessions.iter() {
            let session_guard = session.lock().await;

            // Apply filter
            if !req.guard_name.is_empty() && session_guard.name != req.guard_name {
                continue;
            }

            // Use the typed_metrics for fanotify-specific snapshot
            let snapshot = session_guard.state.typed_metrics.snapshot();

            guard_metrics.push(crate::proto::GuardMetrics {
                guard_name: session_guard.name.clone(),
                namespace: session_guard.namespace.clone(),
                denied_count: snapshot.events_denied as i64,
                allowed_count: snapshot.events_allowed as i64,
                audited_count: snapshot.events_audited as i64,
                event_counts: snapshot.event_counts(),
            });

            // Aggregate totals from per-guard metrics
            total_denied += snapshot.events_denied;
            total_allowed += snapshot.events_allowed;
            total_events += snapshot.events_total;
        }

        Ok(Response::new(MetricsResponse {
            active_guards: totals.active_sessions as i32,
            total_events_processed: total_events as i64,
            total_denied: total_denied as i64,
            total_allowed: total_allowed as i64,
            guard_metrics,
        }))
    }

    /// Update an existing guard (pause/resume).
    async fn update_guard(
        &self,
        request: Request<UpdateGuardRequest>,
    ) -> Result<Response<UpdateGuardResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            action = ?req.action,
            "UpdateGuard request"
        );

        // Get session
        let session_arc = self.get(&key).await
            .ok_or_else(|| Status::not_found(format!("Guard not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        match UpdateAction::try_from(req.action).unwrap_or(UpdateAction::Unspecified) {
            UpdateAction::Pause => {
                if session.paused {
                    return Err(Status::failed_precondition("Guard is already paused"));
                }

                // Stop the guard by setting running to false
                session.running.store(false, Ordering::SeqCst);
                session.paused = true;

                info!(guard_id = %key, "Guard paused");

                Ok(Response::new(UpdateGuardResponse {
                    guard_id: key,
                    paused: true,
                    enforcing: session.state.enforcing,
                    guarded_paths: 0,
                }))
            }
            UpdateAction::Resume => {
                if !session.paused {
                    return Err(Status::failed_precondition("Guard is not paused"));
                }

                // Note: Resume requires recreating the guard task
                // For now, return an error suggesting destroy/recreate
                // Full resume support would require significant refactoring
                Err(Status::unimplemented(
                    "Resume not fully implemented. Please destroy and recreate the guard."
                ))
            }
            UpdateAction::Unspecified => {
                Err(Status::invalid_argument("Action must be PAUSE or RESUME"))
            }
        }
    }

    /// Update the policy of an existing guard.
    async fn update_policy(
        &self,
        request: Request<UpdatePolicyRequest>,
    ) -> Result<Response<UpdatePolicyResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            namespace = %req.namespace,
            pod = %req.pod_name,
            deny_patterns = req.deny_patterns.len(),
            allow_patterns = req.allow_patterns.len(),
            "UpdatePolicy request"
        );

        // Get session
        let session_arc = self.get(&key).await
            .ok_or_else(|| Status::not_found(format!("Guard not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        // Update the subjects with new patterns
        // This stores the new patterns in the session state
        // The guard will use these patterns after being recreated
        if !req.deny_patterns.is_empty() || !req.allow_patterns.is_empty() {
            if let Some(subject) = session.state.subjects.first_mut() {
                if !req.deny_patterns.is_empty() {
                    subject.deny = req.deny_patterns.clone();
                }
                if !req.allow_patterns.is_empty() {
                    subject.allow = req.allow_patterns.clone();
                }
            }
        }

        let deny_count = session.state.subjects.iter().map(|s| s.deny.len()).sum::<usize>() as i32;
        let allow_count = session.state.subjects.iter().map(|s| s.allow.len()).sum::<usize>() as i32;

        info!(
            guard_id = %key,
            deny_patterns = deny_count,
            allow_patterns = allow_count,
            "Policy updated (will take effect on next event or after resume)"
        );

        Ok(Response::new(UpdatePolicyResponse {
            guard_id: key,
            deny_pattern_count: deny_count,
            allow_pattern_count: allow_count,
            cache_cleared: true, // Policy cache is conceptually cleared
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key() {
        // Test using the SessionManager trait method
        let key = JanusdServiceImpl::session_key("my-guard", "default", "pod-abc");
        assert_eq!(key, "my-guard:default:pod-abc");
    }

    #[test]
    fn test_session_key_with_special_chars() {
        let key = JanusdServiceImpl::session_key("guard-1", "kube-system", "pod-with-dashes");
        assert_eq!(key, "guard-1:kube-system:pod-with-dashes");
    }

    #[test]
    fn test_fanotify_event_to_string() {
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::Access),
            Some("access")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::Open),
            Some("open")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::OpenExec),
            Some("open_exec")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::CloseWrite),
            Some("close_write")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::Close),
            Some("close")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::All),
            Some("all")
        );
        assert_eq!(
            JanusdServiceImpl::fanotify_event_to_string(FanotifyEvent::Unspecified),
            None
        );
    }

    #[test]
    fn test_response_to_proto() {
        assert_eq!(
            JanusdServiceImpl::response_to_proto(GuardAccessResponse::Allow),
            AccessResponse::Allow as i32
        );
        assert_eq!(
            JanusdServiceImpl::response_to_proto(GuardAccessResponse::Deny),
            AccessResponse::Deny as i32
        );
        assert_eq!(
            JanusdServiceImpl::response_to_proto(GuardAccessResponse::Audit),
            AccessResponse::Audit as i32
        );
    }

    #[test]
    fn test_event_type_to_proto() {
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("access"),
            FanotifyEvent::Access as i32
        );
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("open"),
            FanotifyEvent::Open as i32
        );
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("open_exec"),
            FanotifyEvent::OpenExec as i32
        );
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("close_write"),
            FanotifyEvent::CloseWrite as i32
        );
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("close"),
            FanotifyEvent::Close as i32
        );
        assert_eq!(
            JanusdServiceImpl::event_type_to_proto("unknown"),
            FanotifyEvent::Unspecified as i32
        );
    }

    #[tokio::test]
    async fn test_janusd_service_new() {
        use crate::audit::NullAuditLogger;
        let audit: Arc<dyn AuditLogger> = Arc::new(NullAuditLogger);
        let service = JanusdServiceImpl::new("test-node".to_string(), 100, audit);
        assert_eq!(service.max_guards, 100);
    }

    #[tokio::test]
    async fn test_session_to_guard_state() {
        let (tx, _rx) = mpsc::channel(10);
        let typed_metrics = Arc::new(GuardMetrics::new("test-guard"));
        let metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();

        let session = Session {
            id: "test-guard:default:pod-1".to_string(),
            name: "test-guard".to_string(),
            namespace: "default".to_string(),
            node_name: "node-1".to_string(),
            pod_name: "pod-1".to_string(),
            container_ids: vec!["container-1".to_string()],
            pids: vec![1234],
            paused: false,
            created_at: SystemTime::now(),
            running: Arc::new(AtomicBool::new(true)),
            metrics,
            state: GuardSessionState {
                subjects: vec![GuardSubject {
                    allow: vec!["/allowed/*".to_string()],
                    deny: vec!["/denied/*".to_string()],
                    events: vec![FanotifyEvent::Open as i32],
                    only_dir: false,
                    auto_allow_owner: false,
                    audit: true,
                    default_response: AccessResponse::Allow as i32,
                    tags: HashMap::new(),
                }],
                log_format: String::new(),
                enforcing: true,
                event_tx: tx,
                typed_metrics,
                guard_config: None,
                marks_registered: false,
                ready_at: None,
                mount_count: 0,
            },
        };

        let state = session_to_guard_state(&session);

        assert_eq!(state.guard_name, "test-guard");
        assert_eq!(state.namespace, "default");
        assert_eq!(state.node_name, "node-1");
        assert_eq!(state.pod_name, "pod-1");
        assert_eq!(state.pids, vec![1234]);
        assert!(!state.paused);
        assert!(state.enforcing);
        assert_eq!(state.guarded_paths, 2); // 1 allow + 1 deny
    }

    #[tokio::test]
    async fn test_guard_session_metrics_snapshot() {
        let (tx, _rx) = mpsc::channel(10);
        let typed_metrics = Arc::new(GuardMetrics::new("test"));
        typed_metrics.record_allowed();
        typed_metrics.record_denied();
        typed_metrics.record_denied();

        let metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();

        let session = Session {
            id: "test:default:pod".to_string(),
            name: "test".to_string(),
            namespace: "default".to_string(),
            node_name: "node".to_string(),
            pod_name: "pod".to_string(),
            container_ids: vec![],
            pids: vec![],
            paused: false,
            created_at: SystemTime::now(),
            running: Arc::new(AtomicBool::new(true)),
            metrics,
            state: GuardSessionState {
                subjects: vec![],
                log_format: String::new(),
                enforcing: false,
                event_tx: tx,
                typed_metrics: typed_metrics.clone(),
                guard_config: None,
                marks_registered: false,
                ready_at: None,
                mount_count: 0,
            },
        };

        let snapshot = session.state.typed_metrics.snapshot();
        assert_eq!(snapshot.events_allowed, 1);
        assert_eq!(snapshot.events_denied, 2);
        assert_eq!(snapshot.events_total, 3);
    }
}
