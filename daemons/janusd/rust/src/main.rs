// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Janus File Access Auditing Daemon - Rust implementation
// Uses nix crate for direct fanotify kernel syscalls

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use panoptes_common::GlogLayer;
use tracing::{info, Level};
use tracing_subscriber::prelude::*;

mod audit;
mod dedupe;
mod guard;
mod metrics;
mod policy;
mod service;

use audit::create_audit_logger;

pub mod proto {
    tonic::include_proto!("janus.v1");
}

/// Service name for health checks (matches the gRPC service name)
const SERVICE_NAME: &str = "janus.v1.JanusdService";

/// Janus daemon configuration
#[derive(Parser, Debug)]
#[command(name = "janusd")]
#[command(version = "2.0.0")]
#[command(about = "File Access Auditing Daemon")]
struct Config {
    /// gRPC listen address (ignored if --port is specified)
    #[arg(long, env = "JANUSD_LISTEN_ADDR", default_value = "0.0.0.0:50052")]
    listen_addr: SocketAddr,

    /// gRPC listen port (overrides listen_addr for C daemon compatibility)
    #[arg(long, env = "JANUSD_PORT")]
    port: Option<u16>,

    /// Kubernetes node name
    #[arg(long, env = "NODE_NAME", default_value = "unknown")]
    node_name: String,

    /// Maximum number of guards
    #[arg(long, env = "JANUSD_MAX_GUARDS", default_value = "1000")]
    max_guards: usize,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    log_level: String,
}

impl Config {
    /// Get the effective listen address, considering --port override
    fn effective_addr(&self) -> SocketAddr {
        if let Some(port) = self.port {
            SocketAddr::from(([0, 0, 0, 0], port))
        } else {
            self.listen_addr
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse configuration
    let config = Config::parse();

    // Initialize logging
    let level = match config.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    // Use glog-compatible format for consistency with C daemon
    tracing_subscriber::registry()
        .with(GlogLayer::new())
        .with(tracing_subscriber::filter::LevelFilter::from_level(level))
        .init();

    // Determine effective listen address (--port overrides --listen-addr)
    let listen_addr = config.effective_addr();

    info!(
        version = "2.0.0",
        node = %config.node_name,
        listen = %listen_addr,
        max_guards = config.max_guards,
        "Starting janusd"
    );

    // Create audit logger (falls back to null logger if CAP_AUDIT_WRITE unavailable)
    let audit_logger: Arc<dyn audit::AuditLogger> = Arc::from(create_audit_logger());
    info!(
        available = audit_logger.is_available(),
        "Audit logger initialized"
    );

    // Create service
    let janusd_service = service::JanusdServiceImpl::new(
        config.node_name.clone(),
        config.max_guards,
        audit_logger,
    );

    // Create health reporter
    let (mut health_reporter, health_service) = health_reporter();

    // Set service as serving
    health_reporter
        .set_serving::<proto::janusd_service_server::JanusdServiceServer<service::JanusdServiceImpl>>()
        .await;

    // Also set the named service for compatibility with grpcurl
    health_reporter.set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving).await;

    // Start gRPC server
    info!(addr = %listen_addr, "Starting gRPC server");

    Server::builder()
        .add_service(health_service)
        .add_service(proto::janusd_service_server::JanusdServiceServer::new(janusd_service))
        .serve(listen_addr)
        .await?;

    Ok(())
}
