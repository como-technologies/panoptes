// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// gRPC service implementation

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::notify::{FileEvent, WatchConfig, Watcher};
use crate::proto::*;

/// Watch session state
struct WatchSession {
    session_id: i64,
    watcher_name: String,
    namespace: String,
    pod_name: String,
    container_id: String,
    watch_count: i32,
    events_detected: AtomicI64,
    created_at: i64,
    running: Arc<std::sync::atomic::AtomicBool>,
}

/// Argus gRPC service implementation
pub struct ArgusServiceImpl {
    node_name: String,
    max_watches: usize,
    next_session_id: AtomicI64,
    sessions: Arc<RwLock<HashMap<i64, Arc<WatchSession>>>>,
    event_tx: mpsc::Sender<crate::proto::FileEvent>,
    event_rx: Arc<Mutex<mpsc::Receiver<crate::proto::FileEvent>>>,
}

impl ArgusServiceImpl {
    pub fn new(node_name: String, max_watches: usize) -> Self {
        let (tx, rx) = mpsc::channel(10000);

        Self {
            node_name,
            max_watches,
            next_session_id: AtomicI64::new(1),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx: tx,
            event_rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[tonic::async_trait]
impl argus_service_server::ArgusService for ArgusServiceImpl {
    async fn create_watch(
        &self,
        request: Request<CreateWatchRequest>,
    ) -> Result<Response<CreateWatchResponse>, Status> {
        let req = request.into_inner();

        info!(
            watcher = %req.watcher_name,
            pod = %req.pod_name,
            container = %req.container_id,
            "CreateWatch request"
        );

        // TODO: Get container PID from container runtime
        // For now, return a placeholder response

        let session_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);

        let session = Arc::new(WatchSession {
            session_id,
            watcher_name: req.watcher_name.clone(),
            namespace: req.namespace.clone(),
            pod_name: req.pod_name.clone(),
            container_id: req.container_id.clone(),
            watch_count: req.paths.len() as i32,
            events_detected: AtomicI64::new(0),
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

        // TODO: Spawn watcher task

        Ok(Response::new(CreateWatchResponse {
            session_id,
            watch_count: req.paths.len() as i32,
            error: String::new(),
        }))
    }

    async fn destroy_watch(
        &self,
        request: Request<DestroyWatchRequest>,
    ) -> Result<Response<DestroyWatchResponse>, Status> {
        let req = request.into_inner();

        info!(session_id = req.session_id, "DestroyWatch request");

        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.remove(&req.session_id) {
            session.running.store(false, Ordering::SeqCst);
            Ok(Response::new(DestroyWatchResponse {
                success: true,
                error: String::new(),
            }))
        } else {
            Ok(Response::new(DestroyWatchResponse {
                success: false,
                error: "Session not found".to_string(),
            }))
        }
    }

    async fn get_watch_status(
        &self,
        request: Request<GetWatchStatusRequest>,
    ) -> Result<Response<GetWatchStatusResponse>, Status> {
        let req = request.into_inner();

        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&req.session_id) {
            Ok(Response::new(GetWatchStatusResponse {
                active: session.running.load(Ordering::SeqCst),
                watch_count: session.watch_count,
                events_detected: session.events_detected.load(Ordering::SeqCst),
                watched_paths: vec![],
                error: String::new(),
            }))
        } else {
            Ok(Response::new(GetWatchStatusResponse {
                active: false,
                watch_count: 0,
                events_detected: 0,
                watched_paths: vec![],
                error: "Session not found".to_string(),
            }))
        }
    }

    async fn list_watches(
        &self,
        request: Request<ListWatchesRequest>,
    ) -> Result<Response<ListWatchesResponse>, Status> {
        let req = request.into_inner();

        let sessions = self.sessions.read().await;

        let watches: Vec<WatchInfo> = sessions
            .values()
            .filter(|s| {
                (req.watcher_name.is_empty() || s.watcher_name == req.watcher_name)
                    && (req.namespace.is_empty() || s.namespace == req.namespace)
            })
            .map(|s| WatchInfo {
                session_id: s.session_id,
                watcher_name: s.watcher_name.clone(),
                namespace: s.namespace.clone(),
                pod_name: s.pod_name.clone(),
                container_id: s.container_id.clone(),
                watch_count: s.watch_count,
                events_detected: s.events_detected.load(Ordering::SeqCst),
                created_at: s.created_at,
            })
            .collect();

        Ok(Response::new(ListWatchesResponse { watches }))
    }

    type StreamEventsStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<crate::proto::FileEvent, Status>> + Send>,
    >;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(1000);

        info!(
            watcher = %req.watcher_name,
            pod = %req.pod_name,
            "StreamEvents started"
        );

        // TODO: Filter and forward events based on request filters

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamEventsStream))
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
