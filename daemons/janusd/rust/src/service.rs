// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// gRPC service implementation for Janus

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::guard::{AccessResponse, GuardConfig};
use crate::proto::*;

/// Guard session state
struct GuardSession {
    session_id: i64,
    guard_name: String,
    namespace: String,
    pod_name: String,
    container_id: String,
    mode: i32,
    allowed_events: AtomicI64,
    denied_events: AtomicI64,
    audited_events: AtomicI64,
    created_at: i64,
    running: Arc<std::sync::atomic::AtomicBool>,
}

/// Janus gRPC service implementation
pub struct JanusServiceImpl {
    node_name: String,
    max_guards: usize,
    audit_enabled: bool,
    next_session_id: AtomicI64,
    sessions: Arc<RwLock<HashMap<i64, Arc<GuardSession>>>>,
}

impl JanusServiceImpl {
    pub fn new(node_name: String, max_guards: usize, audit_enabled: bool) -> Self {
        Self {
            node_name,
            max_guards,
            audit_enabled,
            next_session_id: AtomicI64::new(1),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[tonic::async_trait]
impl janus_service_server::JanusService for JanusServiceImpl {
    async fn create_guard(
        &self,
        request: Request<CreateGuardRequest>,
    ) -> Result<Response<CreateGuardResponse>, Status> {
        let req = request.into_inner();

        info!(
            guard = %req.guard_name,
            pod = %req.pod_name,
            container = %req.container_id,
            "CreateGuard request"
        );

        let session_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);

        let session = Arc::new(GuardSession {
            session_id,
            guard_name: req.guard_name.clone(),
            namespace: req.namespace.clone(),
            pod_name: req.pod_name.clone(),
            container_id: req.container_id.clone(),
            mode: req.mode,
            allowed_events: AtomicI64::new(0),
            denied_events: AtomicI64::new(0),
            audited_events: AtomicI64::new(0),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            running: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        });

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session);
        }

        // TODO: Spawn guard task with fanotify

        Ok(Response::new(CreateGuardResponse {
            session_id,
            error: String::new(),
        }))
    }

    async fn destroy_guard(
        &self,
        request: Request<DestroyGuardRequest>,
    ) -> Result<Response<DestroyGuardResponse>, Status> {
        let req = request.into_inner();

        info!(session_id = req.session_id, "DestroyGuard request");

        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.remove(&req.session_id) {
            session.running.store(false, Ordering::SeqCst);
            Ok(Response::new(DestroyGuardResponse {
                success: true,
                error: String::new(),
            }))
        } else {
            Ok(Response::new(DestroyGuardResponse {
                success: false,
                error: "Session not found".to_string(),
            }))
        }
    }

    async fn get_guard_status(
        &self,
        request: Request<GetGuardStatusRequest>,
    ) -> Result<Response<GetGuardStatusResponse>, Status> {
        let req = request.into_inner();

        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&req.session_id) {
            Ok(Response::new(GetGuardStatusResponse {
                active: session.running.load(Ordering::SeqCst),
                mode: session.mode,
                allowed_events: session.allowed_events.load(Ordering::SeqCst),
                denied_events: session.denied_events.load(Ordering::SeqCst),
                audited_events: session.audited_events.load(Ordering::SeqCst),
                allow_patterns: vec![],
                deny_patterns: vec![],
                error: String::new(),
            }))
        } else {
            Ok(Response::new(GetGuardStatusResponse {
                active: false,
                mode: 0,
                allowed_events: 0,
                denied_events: 0,
                audited_events: 0,
                allow_patterns: vec![],
                deny_patterns: vec![],
                error: "Session not found".to_string(),
            }))
        }
    }

    async fn list_guards(
        &self,
        request: Request<ListGuardsRequest>,
    ) -> Result<Response<ListGuardsResponse>, Status> {
        let req = request.into_inner();

        let sessions = self.sessions.read().await;

        let guards: Vec<GuardInfo> = sessions
            .values()
            .filter(|s| {
                (req.guard_name.is_empty() || s.guard_name == req.guard_name)
                    && (req.namespace.is_empty() || s.namespace == req.namespace)
            })
            .map(|s| GuardInfo {
                session_id: s.session_id,
                guard_name: s.guard_name.clone(),
                namespace: s.namespace.clone(),
                pod_name: s.pod_name.clone(),
                container_id: s.container_id.clone(),
                mode: s.mode,
                allowed_events: s.allowed_events.load(Ordering::SeqCst),
                denied_events: s.denied_events.load(Ordering::SeqCst),
                created_at: s.created_at,
            })
            .collect();

        Ok(Response::new(ListGuardsResponse { guards }))
    }

    type StreamEventsStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<crate::proto::AccessEvent, Status>> + Send>,
    >;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(1000);

        info!(
            guard = %req.guard_name,
            pod = %req.pod_name,
            denied_only = req.denied_only,
            "StreamEvents started"
        );

        // TODO: Filter and forward events based on request filters

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamEventsStream))
    }

    async fn update_policy(
        &self,
        request: Request<UpdatePolicyRequest>,
    ) -> Result<Response<UpdatePolicyResponse>, Status> {
        let req = request.into_inner();

        info!(
            session_id = req.session_id,
            guard = %req.guard_name,
            "UpdatePolicy request"
        );

        // TODO: Update policy in running guard

        Ok(Response::new(UpdatePolicyResponse {
            success: true,
            error: String::new(),
        }))
    }
}

/// Health service implementation
#[derive(Default)]
pub struct HealthServiceImpl;

#[tonic::async_trait]
impl health_service_server::HealthService for HealthServiceImpl {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: health_check_response::ServingStatus::Serving as i32,
        }))
    }

    type WatchStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<HealthCheckResponse, Status>> + Send>,
    >;

    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let (tx, rx) = mpsc::channel(10);

        tokio::spawn(async move {
            loop {
                if tx
                    .send(Ok(HealthCheckResponse {
                        status: health_check_response::ServingStatus::Serving as i32,
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::WatchStream))
    }
}
