// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # JanusdService gRPC Implementation (eBPF Mode)
//!
//! This module implements the gRPC service using eBPF LSM hooks instead of fanotify.
//! It provides full process attribution and kernel-level permission control.
//!
//! ## Differences from Traditional Mode
//!
//! | Aspect | Traditional (fanotify) | eBPF (this module) |
//! |--------|------------------------|---------------------|
//! | Event source | fanotify | LSM hooks |
//! | Process info | /proc lookup (TOCTOU risk) | Atomic (no race) |
//! | Permission control | fanotify_mark + response | DENY_PATHS BPF map |
//! | Filtering | Userspace | Kernel (GUARDED_PREFIXES map) |
//! | Kernel requirement | 2.6.37+ | 5.8+ with BTF |

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use panoptes_common::{
    ContainerRuntime, DaemonMetrics, EventBroadcaster, MetricsAggregator, Session, SessionManager,
    SessionMap, SessionState, detect_runtime,
    ebpf::{EbpfError, EbpfEventReceiver, EbpfLoader},
    new_session_map,
};
use prost_types::Timestamp;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

use crate::audit::{AuditAccessEvent, AuditEventType, AuditLogger};
use crate::ebpf::EbpfAccessEvent;
use crate::metrics::GuardMetrics;
use crate::proto::*;

/// Path to eBPF bytecode
const EBPF_BYTECODE_PATH: &str = "/usr/lib/janusd/janusd-ebpf";

/// LSM programs to attach for access audit events
const LSM_PROGRAMS: &[&str] = &[
    "file_open",       // File open (read/write detection)
    "file_permission", // Permission checks (access audit)
];

/// Janus eBPF session state for file access auditing.
pub struct EbpfGuardSessionState {
    /// Guard subjects (allow/deny patterns)
    pub subjects: Vec<GuardSubject>,
    /// Log format for events
    pub log_format: String,
    /// Whether the guard is enforcing (deny) or just auditing
    pub enforcing: bool,
    /// Typed reference to GuardMetrics
    pub typed_metrics: Arc<GuardMetrics>,
    /// Number of path prefixes in eBPF GUARDED_PREFIXES map
    pub guarded_prefixes: AtomicU64,
    /// Number of deny paths in eBPF DENY_PATHS map
    pub deny_paths_count: AtomicU64,
    /// Whether eBPF filters are configured (guard is ready)
    pub marks_registered: bool,
    /// Timestamp when guard became ready
    pub ready_at: Option<SystemTime>,
    /// Active prefixes for this session (for removal on DestroyGuard)
    pub active_prefixes: Vec<String>,
    /// Active deny paths for this session
    pub active_deny_paths: Vec<String>,
}

impl SessionState for EbpfGuardSessionState {
    type Config = GuardSubject;
    type Event = AccessEvent;
}

// Note: Filterable impl for AccessEvent is in service.rs (shared between modes)

/// Convert a Session<EbpfGuardSessionState> to a proto GuardState message.
fn session_to_guard_state(session: &Session<EbpfGuardSessionState>) -> GuardState {
    let created_at = session
        .created_at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| Timestamp {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
        .ok();

    let ready_at = session.state.ready_at.and_then(|t| {
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
        marks_registered: session.state.marks_registered,
        ready_at,
        mount_count: session.state.guarded_prefixes.load(Ordering::Relaxed) as i32,
    }
}

/// Janus daemon gRPC service implementation (eBPF mode).
pub struct JanusdServiceImpl {
    node_name: String,
    /// Cluster name for multi-cluster deployments.
    cluster_name: String,
    max_guards: usize,
    sessions: SessionMap<EbpfGuardSessionState>,
    broadcaster: EventBroadcaster<AccessEvent>,
    metrics: Arc<MetricsAggregator>,
    runtime: Option<Box<dyn ContainerRuntime>>,
    audit: Arc<dyn AuditLogger>,
    /// Shared eBPF loader (single instance for all sessions)
    ebpf_loader: Arc<Mutex<Option<EbpfLoader>>>,
    /// Event receiver from eBPF
    ebpf_receiver: Arc<Mutex<Option<EbpfEventReceiver>>>,
}

impl SessionManager<EbpfGuardSessionState> for JanusdServiceImpl {
    fn sessions(&self) -> &SessionMap<EbpfGuardSessionState> {
        &self.sessions
    }

    fn max_sessions(&self) -> usize {
        self.max_guards
    }
}

impl JanusdServiceImpl {
    /// Create a new Janusd service instance (eBPF mode).
    pub fn new(
        node_name: String,
        cluster_name: String,
        max_guards: usize,
        audit: Arc<dyn AuditLogger>,
    ) -> Self {
        let runtime: Option<Box<dyn ContainerRuntime>> = detect_runtime();

        if let Some(ref rt) = runtime {
            info!(runtime = ?rt.runtime_type(), "Detected container runtime");
        } else {
            warn!("No container runtime detected");
        }

        Self {
            node_name: node_name.clone(),
            cluster_name,
            max_guards,
            sessions: new_session_map(),
            broadcaster: EventBroadcaster::new(10000),
            metrics: Arc::new(MetricsAggregator::new(&node_name)),
            runtime,
            audit,
            ebpf_loader: Arc::new(Mutex::new(None)),
            ebpf_receiver: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the eBPF subsystem (called on first CreateGuard).
    async fn ensure_ebpf_initialized(&self) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        if loader_guard.is_some() {
            return Ok(());
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
        let audit = self.audit.clone();

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

                // Convert to Janus event
                let ebpf_event = EbpfAccessEvent::from(raw_event);

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

                    // Check if this event matches any guarded prefix
                    let matches = session
                        .state
                        .active_prefixes
                        .iter()
                        .any(|prefix| ebpf_event.path.starts_with(prefix));

                    if !matches {
                        continue;
                    }

                    // Check if path is in deny list
                    let denied = session.state.enforcing
                        && session
                            .state
                            .active_deny_paths
                            .iter()
                            .any(|p| ebpf_event.path == *p || ebpf_event.path.starts_with(p));

                    let response = if denied {
                        AccessResponse::Deny
                    } else {
                        AccessResponse::Allow
                    };

                    let container_id = session
                        .container_ids
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    // Record metrics
                    match response {
                        AccessResponse::Allow => session.state.typed_metrics.record_allowed(),
                        AccessResponse::Deny => session.state.typed_metrics.record_denied(),
                        _ => session.state.typed_metrics.record_audited(),
                    }
                    session
                        .state
                        .typed_metrics
                        .record_event_type(ebpf_event.event_type_str());

                    // Get exe path from cache for logging and audit
                    let exe_path = cached_process
                        .as_ref()
                        .filter(|c| c.has_exe())
                        .map(|c| c.exe_str().to_string())
                        .unwrap_or_default();

                    // Log to daemon stdout with full process attribution
                    let response_str = if denied { "DENY" } else { "ALLOW" };
                    let exe_info = if !exe_path.is_empty() {
                        format!(" ({})", exe_path)
                    } else {
                        String::new()
                    };
                    info!(
                        "<{}> {} '{}' by {}:{}{} ({}:{})",
                        response_str,
                        ebpf_event.event_type_str().to_uppercase(),
                        ebpf_event.path,
                        ebpf_event.comm,
                        ebpf_event.pid,
                        exe_info,
                        session.pod_name,
                        node_name
                    );

                    // Log to kernel audit with process attribution
                    let audit_event = AuditAccessEvent {
                        guard_name: session.name.clone(),
                        namespace: session.namespace.clone(),
                        pod_name: session.pod_name.clone(),
                        container_id: container_id.to_string(),
                        path: ebpf_event.path.clone(),
                        pid: ebpf_event.pid as i32,
                        uid: ebpf_event.uid,
                        gid: ebpf_event.gid,
                        response: if denied {
                            crate::audit::AccessResponse::Deny
                        } else {
                            crate::audit::AccessResponse::Allow
                        },
                        event_type: AuditEventType::Access,
                        comm: ebpf_event.comm.clone(),
                        exe: exe_path.clone(),
                    };
                    if let Err(e) = audit.log_access(&audit_event) {
                        debug!(error = %e, "Failed to log to kernel audit");
                    }

                    // Build ProcessInfo with cached data
                    let process_info = if ebpf_event.has_process_info() {
                        Some(ProcessInfo {
                            pid: ebpf_event.pid as i32,
                            tid: ebpf_event.tgid as i32,
                            uid: ebpf_event.uid as i32,
                            gid: ebpf_event.gid as i32,
                            comm: ebpf_event.comm.clone(),
                            // Use cached process info
                            exe: exe_path,
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
                        })
                    } else {
                        None
                    };

                    let proto_event = AccessEvent {
                        timestamp: Some(Timestamp {
                            seconds: (ebpf_event.timestamp_ns / 1_000_000_000) as i64,
                            nanos: (ebpf_event.timestamp_ns % 1_000_000_000) as i32,
                        }),
                        guard_name: session.name.clone(),
                        namespace: session.namespace.clone(),
                        node_name: node_name.clone(),
                        pod_name: session.pod_name.clone(),
                        container_id: container_id.to_string(),
                        event_type: FanotifyEvent::Access as i32,
                        path: ebpf_event.path.clone(),
                        response: response as i32,
                        process_info,
                        is_directory: false,
                        tags: HashMap::new(),
                        audit_logged: true,
                        // Multi-cluster identification
                        cluster_name: cluster_name.clone(),
                    };

                    let _ = broadcaster.send(proto_event);
                }
            }
        });
    }

    /// Add guarded prefixes to the eBPF filter map.
    async fn add_guarded_prefixes(&self, prefixes: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for prefix in prefixes {
            loader.add_watched_prefix(prefix, "GUARDED_PREFIXES")?;
        }

        Ok(())
    }

    /// Remove guarded prefixes from the eBPF filter map.
    async fn remove_guarded_prefixes(&self, prefixes: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for prefix in prefixes {
            loader.remove_watched_prefix(prefix, "GUARDED_PREFIXES")?;
        }

        Ok(())
    }

    /// Add deny paths to the eBPF DENY_PATHS map (LSM will block access).
    async fn add_deny_paths(&self, paths: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for path in paths {
            loader.add_deny_path(path)?;
        }

        Ok(())
    }

    /// Remove deny paths from the eBPF DENY_PATHS map.
    async fn remove_deny_paths(&self, paths: &[String]) -> Result<(), EbpfError> {
        let mut loader_guard = self.ebpf_loader.lock().await;
        let loader = loader_guard
            .as_mut()
            .ok_or_else(|| EbpfError::Map("eBPF not initialized".into()))?;

        for path in paths {
            loader.remove_deny_path(path)?;
        }

        Ok(())
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

    /// Build path prefixes from guard subjects.
    fn build_prefixes(
        &self,
        subjects: &[GuardSubject],
        container_ids: &[String],
    ) -> (Vec<String>, Vec<String>) {
        let Some(ref runtime) = self.runtime else {
            return (vec![], vec![]);
        };

        let mut all_prefixes = Vec::new();
        let mut deny_paths = Vec::new();

        for container_id in container_ids {
            let Ok(pid) = runtime.resolve_pid(container_id) else {
                continue;
            };
            let container_root = runtime.resolve_container_root(pid);

            for subject in subjects {
                // Allow patterns become watched prefixes
                for path in &subject.allow {
                    let full_path = if let Some(stripped) = path.strip_prefix('/') {
                        container_root.join(stripped)
                    } else {
                        container_root.join(path)
                    };
                    all_prefixes.push(full_path.to_string_lossy().to_string());
                }

                // Deny patterns become DENY_PATHS entries
                for path in &subject.deny {
                    let full_path = if let Some(stripped) = path.strip_prefix('/') {
                        container_root.join(stripped)
                    } else {
                        container_root.join(path)
                    };
                    deny_paths.push(full_path.to_string_lossy().to_string());
                }
            }
        }

        (all_prefixes, deny_paths)
    }
}

#[tonic::async_trait]
impl janusd_service_server::JanusdService for JanusdServiceImpl {
    /// Create a new file access guard (eBPF mode).
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
            "CreateGuard request (eBPF mode)"
        );

        // Check limits
        if self.count().await >= self.max_sessions() {
            return Err(Status::resource_exhausted(format!(
                "Maximum guards ({}) exceeded",
                self.max_guards
            )));
        }
        if self.exists(&key).await {
            return Err(Status::already_exists(format!(
                "Guard already exists: {}",
                key
            )));
        }

        // Initialize eBPF subsystem
        self.ensure_ebpf_initialized()
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize eBPF: {}", e)))?;

        // Resolve container PIDs
        let pids = if req.pids.is_empty() {
            self.resolve_container_pids(&req.container_ids)
        } else {
            req.pids.clone()
        };

        // Build path prefixes from subjects
        let (all_prefixes, deny_paths) = self.build_prefixes(&req.subjects, &req.container_ids);

        let guarded_paths: i32 = req
            .subjects
            .iter()
            .map(|s| (s.allow.len() + s.deny.len()) as i32)
            .sum();

        // Create metrics
        let typed_metrics = Arc::new(GuardMetrics::new(&req.guard_name));
        let metrics: Arc<dyn DaemonMetrics> = typed_metrics.clone();

        // Create session
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
            state: EbpfGuardSessionState {
                subjects: req.subjects.clone(),
                log_format: req.log_format.clone(),
                enforcing: req.enforcing,
                typed_metrics: typed_metrics.clone(),
                guarded_prefixes: AtomicU64::new(all_prefixes.len() as u64),
                deny_paths_count: AtomicU64::new(deny_paths.len() as u64),
                marks_registered: false,
                ready_at: None,
                active_prefixes: all_prefixes.clone(),
                active_deny_paths: deny_paths.clone(),
            },
        }));

        // Insert session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key.clone(), session.clone());
        }

        self.metrics.register(metrics.clone()).await;

        let mut marks_registered: i32 = 0;

        // Configure eBPF filters if not paused
        if !req.paused && !all_prefixes.is_empty() {
            // Add guarded prefixes
            if let Err(e) = self.add_guarded_prefixes(&all_prefixes).await {
                warn!(error = %e, "Failed to configure eBPF prefixes");
            } else {
                marks_registered = all_prefixes.len() as i32;
            }

            // Add deny paths if enforcing
            if req.enforcing
                && !deny_paths.is_empty()
                && let Err(e) = self.add_deny_paths(&deny_paths).await
            {
                warn!(error = %e, "Failed to configure deny paths");
            }

            // Mark as ready
            let ready_at = SystemTime::now();
            {
                let mut session_guard = session.lock().await;
                session_guard.state.marks_registered = true;
                session_guard.state.ready_at = Some(ready_at);
            }

            info!(
                guard_id = %key,
                prefixes = all_prefixes.len(),
                deny_paths = deny_paths.len(),
                "eBPF guard configured"
            );
        }

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

    /// Destroy an existing guard.
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
            "DestroyGuard request (eBPF mode)"
        );

        if let Some(session) = self.remove(&key).await {
            let session = session.lock().await;

            // Remove eBPF filters
            if !session.state.active_prefixes.is_empty()
                && let Err(e) = self
                    .remove_guarded_prefixes(&session.state.active_prefixes)
                    .await
            {
                warn!(error = %e, "Failed to remove eBPF prefixes");
            }
            if !session.state.active_deny_paths.is_empty()
                && let Err(e) = self
                    .remove_deny_paths(&session.state.active_deny_paths)
                    .await
            {
                warn!(error = %e, "Failed to remove deny paths");
            }

            self.metrics.unregister(&key).await;
            info!(guard = %key, "Guard destroyed");
            Ok(Response::new(()))
        } else {
            Err(Status::not_found(format!("Guard not found: {}", key)))
        }
    }

    type GetGuardStateStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<GuardState, Status>> + Send>>;

    async fn get_guard_state(
        &self,
        request: Request<GetGuardStateRequest>,
    ) -> Result<Response<Self::GetGuardStateStream>, Status> {
        let req = request.into_inner();

        let (tx, rx) = mpsc::channel(100);
        let sessions = self.sessions.clone();

        tokio::spawn(async move {
            let sessions = sessions.read().await;
            for session_arc in sessions.values() {
                let session = session_arc.lock().await;

                if !req.guard_name.is_empty() && session.name != req.guard_name {
                    continue;
                }
                if !req.namespace.is_empty() && session.namespace != req.namespace {
                    continue;
                }

                let state = session_to_guard_state(&session);
                drop(session);

                if tx.send(Ok(state)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    type StreamAccessEventsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<AccessEvent, Status>> + Send>>;

    async fn stream_access_events(
        &self,
        request: Request<StreamAccessEventsRequest>,
    ) -> Result<Response<Self::StreamAccessEventsStream>, Status> {
        let req = request.into_inner();

        info!(
            guard_name = %req.guard_name,
            namespace = %req.namespace,
            "StreamAccessEvents started (eBPF mode)"
        );

        let (tx, rx) = mpsc::channel(1000);
        let mut event_rx = self.broadcaster.subscribe();

        let filter_guard_name = req.guard_name;
        let filter_namespace = req.namespace;
        let filter_event_types: Vec<i32> = req.event_types;
        let include_allowed = req.include_allowed;

        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
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

                        if tx.send(Ok(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!("Stream lagged, skipped {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let req = request.into_inner();
        let totals = self.metrics.totals();
        let sessions = self.sessions.read().await;

        let mut guard_metrics = Vec::new();
        let mut total_denied: u64 = 0;
        let mut total_allowed: u64 = 0;
        let mut total_events: u64 = 0;

        for session_arc in sessions.values() {
            let session = session_arc.lock().await;

            if !req.guard_name.is_empty() && session.name != req.guard_name {
                continue;
            }

            let snapshot = session.state.typed_metrics.snapshot();

            guard_metrics.push(crate::proto::GuardMetrics {
                guard_name: session.name.clone(),
                namespace: session.namespace.clone(),
                denied_count: snapshot.events_denied as i64,
                allowed_count: snapshot.events_allowed as i64,
                audited_count: snapshot.events_audited as i64,
                event_counts: snapshot.event_counts(),
                queue_overflows: 0,
            });

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

    async fn update_guard(
        &self,
        request: Request<UpdateGuardRequest>,
    ) -> Result<Response<UpdateGuardResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            action = ?req.action,
            "UpdateGuard request (eBPF mode)"
        );

        let session_arc = self
            .get(&key)
            .await
            .ok_or_else(|| Status::not_found(format!("Guard not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        match UpdateAction::try_from(req.action).unwrap_or(UpdateAction::Unspecified) {
            UpdateAction::Pause => {
                if session.paused {
                    return Err(Status::failed_precondition("Guard is already paused"));
                }

                // Remove eBPF filters
                if !session.state.active_prefixes.is_empty() {
                    self.remove_guarded_prefixes(&session.state.active_prefixes)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to pause: {}", e)))?;
                }
                if !session.state.active_deny_paths.is_empty() {
                    self.remove_deny_paths(&session.state.active_deny_paths)
                        .await
                        .map_err(|e| {
                            Status::internal(format!("Failed to remove deny paths: {}", e))
                        })?;
                }

                session.paused = true;
                session.running.store(false, Ordering::SeqCst);

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

                // Re-add eBPF filters
                if !session.state.active_prefixes.is_empty() {
                    self.add_guarded_prefixes(&session.state.active_prefixes)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to resume: {}", e)))?;
                }
                if session.state.enforcing && !session.state.active_deny_paths.is_empty() {
                    self.add_deny_paths(&session.state.active_deny_paths)
                        .await
                        .map_err(|e| {
                            Status::internal(format!("Failed to add deny paths: {}", e))
                        })?;
                }

                session.paused = false;
                session.running.store(true, Ordering::SeqCst);
                let guarded_paths = session.state.active_prefixes.len() as i32;

                Ok(Response::new(UpdateGuardResponse {
                    guard_id: key,
                    paused: false,
                    enforcing: session.state.enforcing,
                    guarded_paths,
                }))
            }
            UpdateAction::Unspecified => {
                Err(Status::invalid_argument("Action must be PAUSE or RESUME"))
            }
        }
    }

    async fn update_policy(
        &self,
        request: Request<UpdatePolicyRequest>,
    ) -> Result<Response<UpdatePolicyResponse>, Status> {
        let req = request.into_inner();
        let key = Self::session_key(&req.guard_name, &req.namespace, &req.pod_name);

        info!(
            guard = %req.guard_name,
            deny_patterns = req.deny_patterns.len(),
            allow_patterns = req.allow_patterns.len(),
            "UpdatePolicy request (eBPF mode)"
        );

        let session_arc = self
            .get(&key)
            .await
            .ok_or_else(|| Status::not_found(format!("Guard not found: {}", key)))?;

        let mut session = session_arc.lock().await;

        // Update deny paths in eBPF map
        if !req.deny_patterns.is_empty() {
            // Remove old deny paths
            if !session.state.active_deny_paths.is_empty() {
                self.remove_deny_paths(&session.state.active_deny_paths)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("Failed to remove old deny paths: {}", e))
                    })?;
            }

            // Add new deny paths
            self.add_deny_paths(&req.deny_patterns)
                .await
                .map_err(|e| Status::internal(format!("Failed to add deny paths: {}", e)))?;

            session.state.active_deny_paths = req.deny_patterns.clone();
            session
                .state
                .deny_paths_count
                .store(req.deny_patterns.len() as u64, Ordering::Relaxed);

            // Update subjects
            if let Some(subject) = session.state.subjects.first_mut() {
                subject.deny = req.deny_patterns;
            }
        }

        if !req.allow_patterns.is_empty()
            && let Some(subject) = session.state.subjects.first_mut()
        {
            subject.allow = req.allow_patterns;
        }

        let deny_count = session
            .state
            .subjects
            .iter()
            .map(|s| s.deny.len())
            .sum::<usize>() as i32;
        let allow_count = session
            .state
            .subjects
            .iter()
            .map(|s| s.allow.len())
            .sum::<usize>() as i32;

        Ok(Response::new(UpdatePolicyResponse {
            guard_id: key,
            deny_pattern_count: deny_count,
            allow_pattern_count: allow_count,
            cache_cleared: true,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key() {
        let key = JanusdServiceImpl::session_key("my-guard", "default", "pod-abc");
        assert_eq!(key, "my-guard:default:pod-abc");
    }
}
