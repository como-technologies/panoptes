// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Argus File Integrity Monitoring Daemon - Rust implementation
// Uses nix crate for direct inotify kernel syscalls

use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use tonic::transport::Server;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod notify;
mod service;

pub mod proto {
    tonic::include_proto!("argus.v1");
}

/// Argus daemon configuration
#[derive(Parser, Debug)]
#[command(name = "argusd")]
#[command(version = "2.0.0")]
#[command(about = "File Integrity Monitoring Daemon")]
struct Config {
    /// gRPC listen address
    #[arg(long, env = "ARGUSD_LISTEN_ADDR", default_value = "0.0.0.0:50051")]
    listen_addr: SocketAddr,

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

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(true)
        .json()
        .init();

    info!(
        version = "2.0.0",
        node = %config.node_name,
        listen = %config.listen_addr,
        max_watches = config.max_watches,
        "Starting argusd"
    );

    // Create service
    let argus_service = service::ArgusServiceImpl::new(
        config.node_name.clone(),
        config.max_watches,
    );

    let health_service = service::HealthServiceImpl::default();

    // Start gRPC server
    info!(addr = %config.listen_addr, "Starting gRPC server");

    Server::builder()
        .add_service(proto::argus_service_server::ArgusServiceServer::new(argus_service))
        .add_service(proto::health_service_server::HealthServiceServer::new(health_service))
        .serve(config.listen_addr)
        .await?;

    Ok(())
}
