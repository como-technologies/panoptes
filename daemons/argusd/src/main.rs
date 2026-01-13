// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Argus File Integrity Monitoring Daemon - Rust implementation
// Uses nix crate for direct inotify kernel syscalls

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use panoptes_common::{
    GlogLayer,
    // Environment abstraction
    EnvironmentDetector, LinuxEnvironmentDetector, Feature, WarningSeverity,
    // Capability checking
    CapabilityChecker, LinuxCapabilityChecker, ARGUSD_REQUIRED_CAPS,
    missing_capabilities_message,
};
#[cfg(feature = "ebpf")]
use panoptes_common::{is_ebpf_supported, is_bpf_lsm_enabled, ARGUSD_REQUIRED_CAPS_EBPF};
use tracing::{info, warn, error, Level};
use tracing_subscriber::prelude::*;

mod metrics;

// Traditional mode (always compiled)
mod notify;
mod service;

// eBPF mode (conditional compilation)
#[cfg(feature = "ebpf")]
mod ebpf;
#[cfg(feature = "ebpf")]
mod service_ebpf;

pub mod proto {
    // V2 proto types (V1 deprecated - all clients should use V2)
    tonic::include_proto!("argus.v2");
}

/// Service name for health checks (matches the gRPC service name)
const SERVICE_NAME: &str = "argus.v2.ArgusdService";

/// Runtime mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum RuntimeMode {
    Traditional,
    Ebpf,
}

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

    /// Force a specific mode (auto, ebpf, traditional)
    #[arg(long, env = "ARGUSD_MODE", default_value = "auto")]
    mode: String,
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

/// Log why eBPF is not supported (for debugging).
#[cfg(feature = "ebpf")]
fn log_ebpf_unsupported_reason() {
    if !std::path::Path::new("/sys/kernel/btf/vmlinux").exists() {
        warn!("BTF not available at /sys/kernel/btf/vmlinux (kernel too old or BTF disabled)");
    }
    if !is_bpf_lsm_enabled() {
        warn!("BPF LSM not enabled - check /sys/kernel/security/lsm for 'bpf'");
        warn!("Note: WSL2 kernels typically do not have BPF LSM support");
    }
}

/// Detect which runtime mode to use
#[cfg(feature = "ebpf")]
fn detect_runtime_mode(config: &Config, cap_checker: &LinuxCapabilityChecker) -> RuntimeMode {
    // Check if user forced a specific mode
    match config.mode.to_lowercase().as_str() {
        "ebpf" => {
            // Only use eBPF if explicitly requested AND supported
            if !is_ebpf_supported() {
                warn!("eBPF mode requested but not supported - falling back to traditional");
                log_ebpf_unsupported_reason();
                return RuntimeMode::Traditional;
            }
            let missing_caps = cap_checker.check_required(ARGUSD_REQUIRED_CAPS_EBPF);
            if !missing_caps.is_empty() {
                warn!(
                    "eBPF mode requested but missing capabilities ({:?}) - falling back to traditional",
                    missing_caps
                );
                return RuntimeMode::Traditional;
            }
            info!("eBPF mode explicitly enabled via --mode flag");
            return RuntimeMode::Ebpf;
        }
        "traditional" | "inotify" => {
            info!("Traditional mode selected via --mode flag");
            return RuntimeMode::Traditional;
        }
        "auto" | _ => {
            // Auto mode: try eBPF first if supported, fall back to traditional
            if !is_ebpf_supported() {
                info!("Auto mode: eBPF not supported - using traditional (inotify)");
                log_ebpf_unsupported_reason();
                return RuntimeMode::Traditional;
            }
            let missing_caps = cap_checker.check_required(ARGUSD_REQUIRED_CAPS_EBPF);
            if !missing_caps.is_empty() {
                info!(
                    "Auto mode: missing eBPF capabilities ({:?}) - using traditional",
                    missing_caps
                );
                return RuntimeMode::Traditional;
            }
            info!("Auto mode: eBPF supported and capabilities present - using eBPF");
            return RuntimeMode::Ebpf;
        }
    }
}

/// Detect which runtime mode to use (non-eBPF build)
#[cfg(not(feature = "ebpf"))]
fn detect_runtime_mode(config: &Config, _cap_checker: &LinuxCapabilityChecker) -> RuntimeMode {
    if config.mode.to_lowercase() == "ebpf" {
        warn!("eBPF mode requested but binary compiled without eBPF support - using traditional mode");
    }
    RuntimeMode::Traditional
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

    // ═══════════════════════════════════════════════════════════════
    // Environment Detection & Validation
    // ═══════════════════════════════════════════════════════════════
    let env_detector = LinuxEnvironmentDetector::new();
    let environment = env_detector.detect();

    info!(environment = %environment, "Detected deployment environment");

    // Log any environment warnings
    for warning in env_detector.environment_warnings() {
        match warning.severity {
            WarningSeverity::Info => info!("[{}] {}", warning.code, warning.message),
            WarningSeverity::Warning => warn!("[{}] {}", warning.code, warning.message),
            WarningSeverity::Error => error!("[{}] {}", warning.code, warning.message),
        }
    }

    // Validate required features (inotify always needed as fallback)
    env_detector
        .validate_for_feature(Feature::Inotify)
        .context("inotify feature validation failed")?;
    env_detector
        .validate_for_feature(Feature::ProcAccess)
        .context("/proc access validation failed")?;

    // ═══════════════════════════════════════════════════════════════
    // Capability Verification (fail fast if missing base caps)
    // ═══════════════════════════════════════════════════════════════
    let cap_checker = LinuxCapabilityChecker::new();
    let missing_caps = cap_checker.check_required(ARGUSD_REQUIRED_CAPS);

    if !missing_caps.is_empty() {
        let msg = missing_capabilities_message(&missing_caps, "argusd");
        error!("{}", msg);
        anyhow::bail!("Missing required capabilities - daemon cannot start");
    }

    info!("Base capabilities verified");

    // ═══════════════════════════════════════════════════════════════
    // Runtime Mode Selection (auto-detect with fallback)
    // ═══════════════════════════════════════════════════════════════
    let runtime_mode = detect_runtime_mode(&config, &cap_checker);

    let mode_str = match runtime_mode {
        RuntimeMode::Ebpf => {
            info!("eBPF mode: LSM-based file monitoring with process attribution");
            "ebpf (LSM hooks)"
        }
        RuntimeMode::Traditional => {
            info!("Traditional mode: inotify-based file monitoring");
            info!("Note: Process info unavailable (inotify limitation)");
            "traditional (inotify)"
        }
    };

    // ═══════════════════════════════════════════════════════════════
    // Daemon Startup
    // ═══════════════════════════════════════════════════════════════
    let listen_addr = config.effective_addr();

    info!(
        version = "2.0.0",
        node = %config.node_name,
        listen = %listen_addr,
        max_watches = config.max_watches,
        mode = mode_str,
        "Starting argusd"
    );

    // Create health reporter
    let (mut health_reporter, health_service) = health_reporter();

    // Start appropriate service based on runtime mode
    #[cfg(feature = "ebpf")]
    if runtime_mode == RuntimeMode::Ebpf {
        let argusd_service = Arc::new(service_ebpf::ArgusdServiceImpl::new(
            config.node_name.clone(),
            config.max_watches,
        ));

        // Set service as serving
        health_reporter
            .set_serving::<proto::argusd_service_server::ArgusdServiceServer<Arc<service_ebpf::ArgusdServiceImpl>>>()
            .await;
        health_reporter.set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving).await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::argusd_service_server::ArgusdServiceServer::from_arc(argusd_service))
            .serve(listen_addr)
            .await?;
    } else {
        // Traditional mode (inotify)
        let argusd_service = Arc::new(service::ArgusdServiceImpl::new(
            config.node_name.clone(),
            config.max_watches,
        ));

        health_reporter
            .set_serving::<proto::argusd_service_server::ArgusdServiceServer<Arc<service::ArgusdServiceImpl>>>()
            .await;
        health_reporter.set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving).await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::argusd_service_server::ArgusdServiceServer::from_arc(argusd_service))
            .serve(listen_addr)
            .await?;
    }

    #[cfg(not(feature = "ebpf"))]
    {
        // Traditional mode only (no eBPF support compiled in)
        let argusd_service = Arc::new(service::ArgusdServiceImpl::new(
            config.node_name.clone(),
            config.max_watches,
        ));

        health_reporter
            .set_serving::<proto::argusd_service_server::ArgusdServiceServer<Arc<service::ArgusdServiceImpl>>>()
            .await;
        health_reporter.set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving).await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::argusd_service_server::ArgusdServiceServer::from_arc(argusd_service))
            .serve(listen_addr)
            .await?;
    }

    Ok(())
}
