// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Janus File Access Auditing Daemon - Rust implementation
// Uses nix crate for direct fanotify kernel syscalls

use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use tonic::transport::Server;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod guard;
mod service;

pub mod proto {
    tonic::include_proto!("janus.v1");
}

/// Janus daemon configuration
#[derive(Parser, Debug)]
#[command(name = "janusd")]
#[command(version = "2.0.0")]
#[command(about = "File Access Auditing Daemon")]
struct Config {
    /// gRPC listen address
    #[arg(long, env = "JANUSD_LISTEN_ADDR", default_value = "0.0.0.0:50052")]
    listen_addr: SocketAddr,

    /// Kubernetes node name
    #[arg(long, env = "NODE_NAME", default_value = "unknown")]
    node_name: String,

    /// Maximum number of guards
    #[arg(long, env = "JANUSD_MAX_GUARDS", default_value = "1000")]
    max_guards: usize,

    /// Enable kernel audit logging
    #[arg(long, env = "JANUSD_AUDIT_ENABLED", default_value = "false")]
    audit_enabled: bool,

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
        max_guards = config.max_guards,
        audit_enabled = config.audit_enabled,
        "Starting janusd"
    );

    // Create service
    let janus_service = service::JanusServiceImpl::new(
        config.node_name.clone(),
        config.max_guards,
        config.audit_enabled,
    );

    let health_service = service::HealthServiceImpl::default();

    // Start gRPC server
    info!(addr = %config.listen_addr, "Starting gRPC server");

    Server::builder()
        .add_service(proto::janus_service_server::JanusServiceServer::new(janus_service))
        .add_service(proto::health_service_server::HealthServiceServer::new(health_service))
        .serve(config.listen_addr)
        .await?;

    Ok(())
}
