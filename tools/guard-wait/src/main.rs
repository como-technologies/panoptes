// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # guard-wait
//!
//! Init container binary that waits for JanusGuard readiness before allowing
//! the main container to start. This eliminates the race condition where file
//! access could occur before fanotify protection is active.
//!
//! ## Usage
//!
//! ```bash
//! guard-wait --guard-name my-guard --namespace default --pod-name my-pod
//! ```
//!
//! ## Environment Variables
//!
//! All CLI arguments can be set via environment variables:
//! - `GUARD_NAME` - Name of the JanusGuard resource
//! - `NAMESPACE` - Kubernetes namespace
//! - `POD_NAME` - Name of the pod being guarded
//! - `JANUSD_ADDRESS` - Address of janusd service (default: janusd.panoptes-system:50052)
//! - `MAX_WAIT_SECS` - Maximum wait time in seconds (default: 30)
//! - `POLL_INTERVAL_MS` - Poll interval in milliseconds (default: 500)

use clap::Parser;
use std::time::Duration;
use tonic::transport::Channel;
use tracing::{error, info, warn};

// Generated proto code
pub mod proto {
    tonic::include_proto!("janus.v1");
}

use proto::GetGuardStateRequest;
use proto::janusd_service_client::JanusdServiceClient;

/// Init container that waits for JanusGuard readiness
#[derive(Parser, Debug)]
#[command(name = "guard-wait")]
#[command(about = "Wait for JanusGuard to be ready before pod startup")]
#[command(version)]
struct Args {
    /// Name of the JanusGuard resource
    #[arg(long, env = "GUARD_NAME")]
    guard_name: String,

    /// Kubernetes namespace
    #[arg(long, env = "NAMESPACE")]
    namespace: String,

    /// Name of the pod being guarded
    #[arg(long, env = "POD_NAME")]
    pod_name: String,

    /// Address of janusd gRPC service
    #[arg(
        long,
        env = "JANUSD_ADDRESS",
        default_value = "http://janusd.panoptes-system:50052"
    )]
    janusd_address: String,

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
        guard_name = %args.guard_name,
        namespace = %args.namespace,
        pod_name = %args.pod_name,
        janusd = %args.janusd_address,
        timeout_secs = args.max_wait_secs,
        "Waiting for JanusGuard to be ready"
    );

    let result = wait_for_guard(&args).await;

    match result {
        Ok(mount_count) => {
            info!(
                mount_count = mount_count,
                "Guard is READY - fanotify marks registered, pod can start safely"
            );
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Guard did not become ready within timeout");
            error!("Pod startup blocked to prevent unprotected file access");
            error!("Possible causes:");
            error!("  - janusd is not running on this node");
            error!("  - JanusGuard resource is misconfigured");
            error!("  - Container runtime issues preventing PID resolution");
            std::process::exit(1);
        }
    }
}

async fn wait_for_guard(args: &Args) -> Result<i32, Box<dyn std::error::Error>> {
    let max_wait = Duration::from_secs(args.max_wait_secs);
    let poll_interval = Duration::from_millis(args.poll_interval_ms);
    let start = std::time::Instant::now();

    loop {
        // Check timeout
        if start.elapsed() >= max_wait {
            return Err(format!(
                "Timeout after {}s waiting for guard '{}' in namespace '{}'",
                args.max_wait_secs, args.guard_name, args.namespace
            )
            .into());
        }

        // Try to connect and query guard state
        match query_guard_state(args).await {
            Ok(Some((ready, mount_count))) => {
                if ready {
                    return Ok(mount_count);
                }
                info!(
                    elapsed_secs = start.elapsed().as_secs(),
                    "Guard exists but marks not yet registered, waiting..."
                );
            }
            Ok(None) => {
                info!(
                    elapsed_secs = start.elapsed().as_secs(),
                    pod_name = %args.pod_name,
                    "Guard for pod not found yet, waiting..."
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    elapsed_secs = start.elapsed().as_secs(),
                    "Failed to query janusd, will retry..."
                );
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Query janusd for guard state, returns (marks_registered, mount_count) if found
async fn query_guard_state(args: &Args) -> Result<Option<(bool, i32)>, Box<dyn std::error::Error>> {
    // Connect to janusd
    let channel = Channel::from_shared(args.janusd_address.clone())?
        .connect_timeout(Duration::from_secs(5))
        .connect()
        .await?;

    let mut client = JanusdServiceClient::new(channel);

    // Query guard state
    let request = tonic::Request::new(GetGuardStateRequest {
        guard_name: args.guard_name.clone(),
        namespace: args.namespace.clone(),
    });

    let mut stream = client.get_guard_state(request).await?.into_inner();

    // Process streaming response - look for our pod's guard
    while let Some(state) = stream.message().await? {
        if state.pod_name == args.pod_name {
            return Ok(Some((state.marks_registered, state.mount_count)));
        }
    }

    // Guard for this pod not found
    Ok(None)
}
