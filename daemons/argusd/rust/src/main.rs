// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Argus File Integrity Monitoring Daemon - Rust implementation
// Uses nix crate for direct inotify kernel syscalls

use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use panoptes_common::GlogLayer;
use tracing::{info, Level};
use tracing_subscriber::prelude::*;

mod metrics;
mod notify;
mod service;

pub mod proto {
    tonic::include_proto!("argus.v1");
}

/// Service name for health checks (matches the gRPC service name)
const SERVICE_NAME: &str = "argus.v1.ArgusdService";

/// Argus daemon configuration
#[derive(Parser, Debug)]
#[command(name = "argusd")]
#[command(version = "2.0.0")]
#[command(about = "File Integrity Monitoring Daemon")]
struct Config {
    /// gRPC listen address (ignored if --port is specified)
    #[arg(long, env = "ARGUSD_LISTEN_ADDR", default_value = "0.0.0.0:50051")]
    listen_addr: SocketAddr,

    /// gRPC listen port (overrides listen_addr for C daemon compatibility)
    #[arg(long, env = "ARGUSD_PORT")]
    port: Option<u16>,

    /// Kubernetes node name
    #[arg(long, env = "NODE_NAME", default_value = "unknown")]
    node_name: String,

    /// Maximum number of watches
    #[arg(long, env = "ARGUSD_MAX_WATCHES", default_value = "10000")]
    max_watches: usize,

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
        max_watches = config.max_watches,
        "Starting argusd"
    );

    // Create service
    let argusd_service = service::ArgusdServiceImpl::new(
        config.node_name.clone(),
        config.max_watches,
    );

    // Create health reporter
    let (mut health_reporter, health_service) = health_reporter();

    // Set service as serving
    health_reporter
        .set_serving::<proto::argusd_service_server::ArgusdServiceServer<service::ArgusdServiceImpl>>()
        .await;

    // Also set the named service for compatibility with grpcurl
    health_reporter.set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving).await;

    // Start gRPC server
    info!(addr = %listen_addr, "Starting gRPC server");

    Server::builder()
        .add_service(health_service)
        .add_service(proto::argusd_service_server::ArgusdServiceServer::new(argusd_service))
        .serve(listen_addr)
        .await?;

    Ok(())
}
