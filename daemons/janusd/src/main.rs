// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
// Janus File Access Auditing Daemon - Rust implementation
// Uses nix crate for direct fanotify kernel syscalls

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use panoptes_common::{
    // Capability checking
    CapabilityChecker,
    // Environment abstraction
    EnvironmentDetector,
    Feature,
    GlogLayer,
    JANUSD_REQUIRED_CAPS,
    LinuxCapabilityChecker,
    LinuxEnvironmentDetector,
    ResourceLimitsInfo,
    WarningSeverity,
    // Resource limit checking
    check_fd_limit,
    missing_capabilities_message,
    read_fanotify_limits,
};
#[cfg(feature = "ebpf")]
use panoptes_common::{JANUSD_REQUIRED_CAPS_EBPF, is_bpf_lsm_enabled, is_ebpf_supported};
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use tracing::{Level, error, info, warn};
use tracing_subscriber::prelude::*;

mod audit;
mod metrics;

// Traditional mode (always compiled)
mod dedupe;
mod guard;
mod policy;
mod service;

// eBPF mode (conditional compilation)
#[cfg(feature = "ebpf")]
mod ebpf;
#[cfg(feature = "ebpf")]
mod service_ebpf;

use audit::create_audit_logger;

pub mod proto {
    // V2 proto types (V1 deprecated - all clients should use V2)
    tonic::include_proto!("janus.v2");
}

/// Service name for health checks (matches the gRPC service name)
const SERVICE_NAME: &str = "janus.v2.JanusdService";

/// Runtime mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum RuntimeMode {
    Traditional,
    Ebpf,
}

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

    /// Force a specific mode (auto, ebpf, traditional)
    #[arg(long, env = "JANUSD_MODE", default_value = "auto")]
    mode: String,

    /// Cluster name for multi-cluster deployments
    #[arg(long, env = "PANOPTES_CLUSTER_NAME", default_value = "")]
    cluster_name: String,
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
            let missing_caps = cap_checker.check_required(JANUSD_REQUIRED_CAPS_EBPF);
            if !missing_caps.is_empty() {
                warn!(
                    "eBPF mode requested but missing capabilities ({:?}) - falling back to traditional",
                    missing_caps
                );
                return RuntimeMode::Traditional;
            }
            info!("eBPF mode explicitly enabled via --mode flag");
            RuntimeMode::Ebpf
        }
        "traditional" | "fanotify" => {
            info!("Traditional mode selected via --mode flag");
            RuntimeMode::Traditional
        }
        _ => {
            // Auto mode: try eBPF first if supported, fall back to traditional
            if !is_ebpf_supported() {
                info!("Auto mode: eBPF not supported - using traditional (fanotify)");
                log_ebpf_unsupported_reason();
                return RuntimeMode::Traditional;
            }
            let missing_caps = cap_checker.check_required(JANUSD_REQUIRED_CAPS_EBPF);
            if !missing_caps.is_empty() {
                info!(
                    "Auto mode: missing eBPF capabilities ({:?}) - using traditional",
                    missing_caps
                );
                return RuntimeMode::Traditional;
            }
            info!("Auto mode: eBPF supported and capabilities present - using eBPF");
            RuntimeMode::Ebpf
        }
    }
}

/// Detect which runtime mode to use (non-eBPF build)
#[cfg(not(feature = "ebpf"))]
fn detect_runtime_mode(config: &Config, _cap_checker: &LinuxCapabilityChecker) -> RuntimeMode {
    if config.mode.to_lowercase() == "ebpf" {
        warn!(
            "eBPF mode requested but binary compiled without eBPF support - using traditional mode"
        );
    }
    RuntimeMode::Traditional
}

/// Parse log level string to tracing Level.
fn parse_log_level(level_str: &str) -> Level {
    match level_str.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}

/// Initialize tracing with glog-compatible format.
fn setup_logging(level: Level) {
    tracing_subscriber::registry()
        .with(GlogLayer::new())
        .with(tracing_subscriber::filter::LevelFilter::from_level(level))
        .init();
}

/// Validate the deployment environment for required features.
fn validate_environment(env_detector: &LinuxEnvironmentDetector) -> Result<()> {
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

    // Validate required features (fanotify always needed as fallback)
    env_detector
        .validate_for_feature(Feature::Fanotify)
        .context("fanotify feature validation failed")?;
    env_detector
        .validate_for_feature(Feature::ProcAccess)
        .context("/proc access validation failed")?;

    Ok(())
}

/// Verify required Linux capabilities are present.
fn validate_capabilities(cap_checker: &LinuxCapabilityChecker) -> Result<()> {
    let missing_caps = cap_checker.check_required(JANUSD_REQUIRED_CAPS);

    if !missing_caps.is_empty() {
        let msg = missing_capabilities_message(&missing_caps, "janusd");
        error!("{}", msg);
        anyhow::bail!("Missing required capabilities - daemon cannot start");
    }

    info!("Base capabilities verified");
    Ok(())
}

/// Safety margin for file descriptors.
/// Larger than argusd because fanotify events consume FDs - each concurrent
/// event holds an open FD until response is written.
const FD_SAFETY_MARGIN: u64 = 4096;

/// Verify system resource limits are sufficient.
///
/// Checks file descriptor limits and fanotify-specific limits.
/// Fails fast if limits would cause cryptic failures later.
fn validate_resource_limits(max_guards: usize) -> Result<()> {
    // Log all resource limits for diagnostics
    let limits_info = ResourceLimitsInfo::collect();
    limits_info.log();

    // Check file descriptor limit
    if let Err(e) = check_fd_limit(max_guards as u64 * 100, FD_SAFETY_MARGIN) {
        error!("{}", e);
        error!(
            "Increase with: ulimit -n {} (or update container securityContext)",
            max_guards as u64 * 100 + FD_SAFETY_MARGIN
        );
        anyhow::bail!("Insufficient file descriptor limit - daemon cannot start safely");
    }
    info!(max_guards = max_guards, "File descriptor limit verified");

    // Check fanotify-specific limits
    validate_fanotify_limits(max_guards);

    Ok(())
}

/// Check fanotify-specific kernel limits and warn if insufficient.
fn validate_fanotify_limits(max_guards: usize) {
    let (max_user_marks, max_queued_events) = read_fanotify_limits();

    // Note: max_user_marks sysctl may not exist on older kernels
    if max_user_marks > 0 && max_user_marks < max_guards as u64 * 10 {
        warn!(
            kernel_limit = max_user_marks,
            estimated_need = max_guards * 10,
            "max_user_marks may be insufficient for configured max_guards"
        );
        warn!(
            "Fix with: sysctl -w fs.fanotify.max_user_marks={}",
            max_guards * 10
        );
    }

    // Warn if queue is small (increases overflow risk)
    // This is CRITICAL for janusd - queue overflow means missed access events
    if max_queued_events < 32768 {
        warn!(
            current = max_queued_events,
            recommended = 65536,
            "fanotify max_queued_events is low - HIGH risk of queue overflow under load"
        );
        warn!("Fix with: sysctl -w fs.fanotify.max_queued_events=65536");
        warn!("Queue overflow causes SILENT event loss - critical for security monitoring!");
    }
}

/// Log runtime mode and return mode string for startup info.
fn log_runtime_mode(mode: RuntimeMode) -> &'static str {
    match mode {
        RuntimeMode::Ebpf => {
            info!("eBPF mode: LSM-based file access auditing with atomic process attribution");
            info!("Permission control via LSM hooks (deny paths supported)");
            "ebpf (LSM hooks)"
        }
        RuntimeMode::Traditional => {
            info!("Traditional mode: fanotify-based file access auditing");
            info!(
                "Process info via /proc lookups (TOCTOU race possible for short-lived processes)"
            );
            "traditional (fanotify)"
        }
    }
}

/// Log daemon startup information.
fn log_startup_info(config: &Config, listen_addr: SocketAddr, mode_str: &str) {
    info!(
        version = "2.0.0",
        node = %config.node_name,
        cluster = %config.cluster_name,
        listen = %listen_addr,
        max_guards = config.max_guards,
        mode = mode_str,
        "Starting janusd"
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse configuration
    let config = Config::parse();

    // Initialize logging
    let level = parse_log_level(&config.log_level);
    setup_logging(level);

    // Environment detection & validation
    let env_detector = LinuxEnvironmentDetector::new();
    validate_environment(&env_detector)?;

    // Capability verification (fail fast if missing base caps)
    let cap_checker = LinuxCapabilityChecker::new();
    validate_capabilities(&cap_checker)?;

    // Resource limit verification (fail fast if insufficient)
    validate_resource_limits(config.max_guards)?;

    // Runtime mode selection (auto-detect with fallback)
    let runtime_mode = detect_runtime_mode(&config, &cap_checker);
    let mode_str = log_runtime_mode(runtime_mode);

    // Log startup info
    let listen_addr = config.effective_addr();
    log_startup_info(&config, listen_addr, mode_str);

    // Create audit logger (falls back to null logger if CAP_AUDIT_WRITE unavailable)
    let audit_logger: Arc<dyn audit::AuditLogger> = Arc::from(create_audit_logger());
    info!(
        available = audit_logger.is_available(),
        "Audit logger initialized"
    );

    // Create health reporter
    let (health_reporter, health_service) = health_reporter();

    // Start appropriate service based on runtime mode
    #[cfg(feature = "ebpf")]
    if runtime_mode == RuntimeMode::Ebpf {
        let janusd_service = Arc::new(service_ebpf::JanusdServiceImpl::new(
            config.node_name.clone(),
            config.cluster_name.clone(),
            config.max_guards,
            audit_logger,
        ));

        // Set service as serving
        health_reporter
            .set_serving::<proto::janusd_service_server::JanusdServiceServer<Arc<service_ebpf::JanusdServiceImpl>>>()
            .await;
        health_reporter
            .set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving)
            .await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::janusd_service_server::JanusdServiceServer::from_arc(
                janusd_service,
            ))
            .serve(listen_addr)
            .await?;
    } else {
        // Traditional mode (fanotify)
        let janusd_service = Arc::new(service::JanusdServiceImpl::new(
            config.node_name.clone(),
            config.cluster_name.clone(),
            config.max_guards,
            audit_logger,
        ));

        health_reporter
            .set_serving::<proto::janusd_service_server::JanusdServiceServer<Arc<service::JanusdServiceImpl>>>()
            .await;
        health_reporter
            .set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving)
            .await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::janusd_service_server::JanusdServiceServer::from_arc(
                janusd_service,
            ))
            .serve(listen_addr)
            .await?;
    }

    #[cfg(not(feature = "ebpf"))]
    {
        // Traditional mode only (no eBPF support compiled in)
        let janusd_service = Arc::new(service::JanusdServiceImpl::new(
            config.node_name.clone(),
            config.cluster_name.clone(),
            config.max_guards,
            audit_logger,
        ));

        health_reporter
            .set_serving::<proto::janusd_service_server::JanusdServiceServer<Arc<service::JanusdServiceImpl>>>()
            .await;
        health_reporter
            .set_service_status(SERVICE_NAME, tonic_health::ServingStatus::Serving)
            .await;

        info!(addr = %listen_addr, "Starting gRPC server (v2)");

        Server::builder()
            .add_service(health_service)
            .add_service(proto::janusd_service_server::JanusdServiceServer::from_arc(
                janusd_service,
            ))
            .serve(listen_addr)
            .await?;
    }

    Ok(())
}
