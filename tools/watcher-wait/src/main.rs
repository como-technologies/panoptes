// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # watcher-wait
//!
//! Init container binary that waits for ArgusWatcher readiness before allowing
//! the main container to start. This eliminates the race condition where file
//! modification could occur before inotify protection is active.
//!
//! ## Usage
//!
//! ```bash
//! watcher-wait --watcher-name my-watcher --namespace default --pod-name my-pod
//! ```
//!
//! ## Environment Variables
//!
//! All CLI arguments can be set via environment variables:
//! - `WATCHER_NAME` - Name of the ArgusWatcher resource
//! - `NAMESPACE` - Kubernetes namespace
//! - `POD_NAME` - Name of the pod being watched
//! - `ARGUSD_ADDRESS` - Address of argusd service (default: argusd.panoptes-system:50051)
//! - `MAX_WAIT_SECS` - Maximum wait time in seconds (default: 30)
//! - `POLL_INTERVAL_MS` - Poll interval in milliseconds (default: 500)
//!
//! ## Defense Layers
//!
//! This tool is part of the ArgusWatcher hardening pattern:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Pod Startup Flow (Hardened)                   │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  1. Pod CREATE request → Admission Webhook                       │
//! │  2. Webhook injects watcher-wait init container                  │
//! │  3. Pod scheduled, init container starts                         │
//! │  4. watcher-wait polls GetWatchState RPC                         │
//! │  5. Argusd creates watch SYNCHRONOUSLY (watches initialized)     │
//! │  6. GetWatchState returns watches_ready=true                     │
//! │  7. watcher-wait exits 0 → main containers start (PROTECTED)    │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use clap::Parser;
use std::time::Duration;
use tonic::transport::Channel;
use tracing::{error, info, warn};

// Generated proto code
pub mod proto {
    tonic::include_proto!("argus.v1");
}

use proto::GetWatchStateRequest;
use proto::argusd_service_client::ArgusdServiceClient;

/// Init container that waits for ArgusWatcher readiness
#[derive(Parser, Debug)]
#[command(name = "watcher-wait")]
#[command(about = "Wait for ArgusWatcher to be ready before pod startup")]
#[command(version)]
struct Args {
    /// Name of the ArgusWatcher resource
    #[arg(long, env = "WATCHER_NAME")]
    watcher_name: String,

    /// Kubernetes namespace
    #[arg(long, env = "NAMESPACE")]
    namespace: String,

    /// Name of the pod being watched
    #[arg(long, env = "POD_NAME")]
    pod_name: String,

    /// Address of argusd gRPC service
    #[arg(
        long,
        env = "ARGUSD_ADDRESS",
        default_value = "http://argusd.panoptes-system:50051"
    )]
    argusd_address: String,

    /// Maximum wait time in seconds
    #[arg(long, env = "MAX_WAIT_SECS", default_value = "30")]
    max_wait_secs: u64,

    /// Poll interval in milliseconds
    #[arg(long, env = "POLL_INTERVAL_MS", default_value = "500")]
    poll_interval_ms: u64,

    /// Enable verbose logging
    #[arg(long, short, env = "VERBOSE")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let filter = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    info!(
        watcher_name = %args.watcher_name,
        namespace = %args.namespace,
        pod_name = %args.pod_name,
        argusd = %args.argusd_address,
        timeout_secs = args.max_wait_secs,
        "Waiting for ArgusWatcher to be ready"
    );

    let result = wait_for_watcher(&args).await;

    match result {
        Ok(watch_descriptors) => {
            info!(
                watch_descriptors = watch_descriptors,
                "Watcher is READY - inotify watches registered, pod can start safely"
            );
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Watcher did not become ready within timeout");
            error!("Pod startup blocked to prevent unprotected file access");
            error!("Possible causes:");
            error!("  - argusd is not running on this node");
            error!("  - ArgusWatcher resource is misconfigured");
            error!("  - Container runtime issues preventing PID resolution");
            std::process::exit(1);
        }
    }
}

async fn wait_for_watcher(args: &Args) -> Result<i32, Box<dyn std::error::Error>> {
    let max_wait = Duration::from_secs(args.max_wait_secs);
    let poll_interval = Duration::from_millis(args.poll_interval_ms);
    let start = std::time::Instant::now();

    loop {
        // Check timeout
        if start.elapsed() >= max_wait {
            return Err(format!(
                "Timeout after {}s waiting for watcher '{}' in namespace '{}'",
                args.max_wait_secs, args.watcher_name, args.namespace
            )
            .into());
        }

        // Try to connect and query watch state
        match query_watch_state(args).await {
            Ok(Some((ready, watch_descriptors))) => {
                if ready {
                    return Ok(watch_descriptors);
                }
                info!(
                    elapsed_secs = start.elapsed().as_secs(),
                    "Watcher exists but watches not yet registered, waiting..."
                );
            }
            Ok(None) => {
                info!(
                    elapsed_secs = start.elapsed().as_secs(),
                    pod_name = %args.pod_name,
                    "Watcher for pod not found yet, waiting..."
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    elapsed_secs = start.elapsed().as_secs(),
                    "Failed to query argusd, will retry..."
                );
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Query argusd for watch state, returns (watches_ready, active_watch_descriptors) if found
async fn query_watch_state(args: &Args) -> Result<Option<(bool, i32)>, Box<dyn std::error::Error>> {
    // Connect to argusd
    let channel = Channel::from_shared(args.argusd_address.clone())?
        .connect_timeout(Duration::from_secs(5))
        .connect()
        .await?;

    let mut client = ArgusdServiceClient::new(channel);

    // Query watch state
    let request = tonic::Request::new(GetWatchStateRequest {
        watcher_name: args.watcher_name.clone(),
        namespace: args.namespace.clone(),
    });

    let mut stream = client.get_watch_state(request).await?.into_inner();

    // Process streaming response - look for our pod's watcher
    while let Some(state) = stream.message().await? {
        if state.pod_name == args.pod_name {
            return Ok(Some((state.watches_ready, state.active_watch_descriptors)));
        }
    }

    // Watcher for this pod not found
    Ok(None)
}
