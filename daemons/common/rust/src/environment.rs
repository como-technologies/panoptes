//! # Environment Detection and Validation
//!
//! This module provides traits and implementations for detecting and validating
//! the deployment environment of Panoptes daemons.
//!
//! ## Design Philosophy
//!
//! Environment-specific code should NOT be scattered throughout daemon logic.
//! Instead, all environment detection, validation, and warnings are centralized
//! here. Daemons use these traits at startup to:
//!
//! 1. Detect their deployment environment (container, host, etc.)
//! 2. Validate that required features are available
//! 3. Surface any environment-specific warnings
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::environment::{
//!     LinuxEnvironmentDetector, EnvironmentDetector, Feature, WarningSeverity
//! };
//!
//! let detector = LinuxEnvironmentDetector::new();
//! let env = detector.detect();
//! println!("Environment: {:?}", env);
//!
//! // Check for warnings
//! for warning in detector.environment_warnings() {
//!     println!("[{}] {}: {}", warning.severity, warning.code, warning.message);
//! }
//!
//! // Validate features before use
//! detector.validate_for_feature(Feature::Inotify)?;
//! # Ok::<(), panoptes_common::environment::EnvironmentError>(())
//! ```

use std::fmt;
use std::path::Path;
use thiserror::Error;

/// Deployment environment classification.
///
/// Panoptes daemons can run in different environments with different capabilities
/// and limitations. This enum captures the primary deployment modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentEnvironment {
    /// Running directly on host (not in container).
    /// Full access to host filesystem and kernel features.
    Host,

    /// Running in container with host filesystem access.
    /// Typical Kubernetes DaemonSet deployment with /host mount.
    /// Some kernel features may be limited (e.g., audit rules for /proc paths).
    ContainerWithHostAccess,

    /// Running in container without host filesystem access.
    /// Very limited - mainly for testing.
    ContainerIsolated,
}

impl fmt::Display for DeploymentEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host => write!(f, "Host"),
            Self::ContainerWithHostAccess => write!(f, "Container (with host access)"),
            Self::ContainerIsolated => write!(f, "Container (isolated)"),
        }
    }
}

/// Features that may or may not be available depending on environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// inotify file change notifications (Argus).
    /// Available in all Linux environments.
    Inotify,

    /// fanotify file access notifications (Janus).
    /// Requires CAP_SYS_ADMIN.
    Fanotify,

    /// fanotify permission events (allow/deny).
    /// Requires CAP_SYS_ADMIN and specific kernel support.
    FanotifyPermission,

    /// Linux audit netlink subsystem.
    /// Does NOT work for /proc/<pid>/root/* paths in containers.
    AuditNetlink,

    /// /proc filesystem access for process information.
    /// Available in all Linux environments.
    ProcAccess,

    /// Basic eBPF support.
    /// Requires kernel 4.4+ and CAP_BPF (or CAP_SYS_ADMIN).
    Ebpf,

    /// eBPF LSM (Linux Security Module) hooks.
    /// Requires kernel 5.7+ with CONFIG_BPF_LSM=y.
    /// Used for file integrity monitoring with process attribution.
    EbpfLsm,
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inotify => write!(f, "inotify"),
            Self::Fanotify => write!(f, "fanotify"),
            Self::FanotifyPermission => write!(f, "fanotify (permission events)"),
            Self::AuditNetlink => write!(f, "audit netlink"),
            Self::ProcAccess => write!(f, "/proc filesystem"),
            Self::Ebpf => write!(f, "eBPF"),
            Self::EbpfLsm => write!(f, "eBPF LSM hooks"),
        }
    }
}

/// Severity of environment warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Informational - no action needed.
    Info,
    /// Warning - some features may be limited.
    Warning,
    /// Error - critical limitation, feature will not work.
    Error,
}

impl fmt::Display for WarningSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// An environment-specific warning or limitation.
#[derive(Debug, Clone)]
pub struct EnvironmentWarning {
    /// Short code for the warning (e.g., "WSL_DETECTED").
    pub code: &'static str,
    /// Human-readable description.
    pub message: String,
    /// Severity level.
    pub severity: WarningSeverity,
}

/// Errors from environment detection or validation.
#[derive(Debug, Error)]
pub enum EnvironmentError {
    /// Feature is not available in this environment.
    #[error("Feature '{feature}' is not available: {reason}")]
    FeatureUnavailable {
        feature: Feature,
        reason: String,
    },

    /// Environment detection failed.
    #[error("Failed to detect environment: {0}")]
    DetectionFailed(String),
}

/// Trait for detecting and validating deployment environments.
///
/// Implementations of this trait encapsulate all environment-specific detection
/// logic, keeping daemon code clean of scattered environment checks.
pub trait EnvironmentDetector: Send + Sync {
    /// Detect the current deployment environment.
    fn detect(&self) -> DeploymentEnvironment;

    /// Check if host filesystem can be accessed.
    fn can_access_host(&self) -> bool;

    /// Get all warnings for the current environment.
    ///
    /// This should be called at daemon startup and warnings logged appropriately.
    fn environment_warnings(&self) -> Vec<EnvironmentWarning>;

    /// Validate that a feature is available in this environment.
    ///
    /// Returns `Ok(())` if the feature can be used, or `Err` with details
    /// about why it's not available.
    fn validate_for_feature(&self, feature: Feature) -> Result<(), EnvironmentError>;
}

/// Linux-specific environment detector.
///
/// Detects:
/// - WSL (Windows Subsystem for Linux) environment
/// - Container vs host deployment
/// - Host filesystem access (/host mount)
pub struct LinuxEnvironmentDetector {
    /// Cached environment detection result.
    environment: DeploymentEnvironment,
    /// Whether we're running in WSL.
    is_wsl: bool,
    /// Path to host root (usually "/" or "/host").
    host_root: Option<std::path::PathBuf>,
}

impl LinuxEnvironmentDetector {
    /// Create a new Linux environment detector.
    ///
    /// This performs detection once at creation time.
    pub fn new() -> Self {
        let is_wsl = Self::detect_wsl();
        let host_root = Self::detect_host_root();
        let environment = Self::detect_environment(&host_root);

        Self {
            environment,
            is_wsl,
            host_root,
        }
    }

    /// Detect if running in WSL (Windows Subsystem for Linux).
    fn detect_wsl() -> bool {
        std::fs::read_to_string("/proc/version")
            .map(|v| {
                let lower = v.to_lowercase();
                lower.contains("microsoft") || lower.contains("wsl")
            })
            .unwrap_or(false)
    }

    /// Detect host root path.
    ///
    /// In containers with host mounts, this is typically "/host".
    /// Returns None if no host root is accessible.
    fn detect_host_root() -> Option<std::path::PathBuf> {
        // Check environment variable first
        if let Ok(path) = std::env::var("HOST_ROOT_PATH") {
            let p = std::path::PathBuf::from(&path);
            if p.exists() && p.is_dir() {
                return Some(p);
            }
        }

        // Check common host mount path
        let host_path = Path::new("/host");
        if host_path.exists() && host_path.is_dir() {
            return Some(host_path.to_path_buf());
        }

        // No host root - either running on host directly or isolated container
        None
    }

    /// Detect deployment environment.
    fn detect_environment(host_root: &Option<std::path::PathBuf>) -> DeploymentEnvironment {
        // Check if we're in a container by looking for container-specific markers
        let in_container = Self::is_in_container();

        if !in_container {
            return DeploymentEnvironment::Host;
        }

        // We're in a container - check if we have host access
        if host_root.is_some() {
            DeploymentEnvironment::ContainerWithHostAccess
        } else {
            DeploymentEnvironment::ContainerIsolated
        }
    }

    /// Check if running inside a container.
    fn is_in_container() -> bool {
        // Method 1: Check for /.dockerenv
        if Path::new("/.dockerenv").exists() {
            return true;
        }

        // Method 2: Check cgroup for container indicators
        if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
            if cgroup.contains("/docker/")
                || cgroup.contains("/kubepods/")
                || cgroup.contains("/containerd/")
                || cgroup.contains("/crio/")
            {
                return true;
            }
        }

        // Method 3: Check for kubernetes service account
        if Path::new("/var/run/secrets/kubernetes.io").exists() {
            return true;
        }

        false
    }

    /// Check if WSL was detected.
    pub fn is_wsl(&self) -> bool {
        self.is_wsl
    }

    /// Get the detected host root path.
    pub fn host_root(&self) -> Option<&Path> {
        self.host_root.as_deref()
    }
}

impl Default for LinuxEnvironmentDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentDetector for LinuxEnvironmentDetector {
    fn detect(&self) -> DeploymentEnvironment {
        self.environment
    }

    fn can_access_host(&self) -> bool {
        self.host_root.is_some() || self.environment == DeploymentEnvironment::Host
    }

    fn environment_warnings(&self) -> Vec<EnvironmentWarning> {
        let mut warnings = Vec::new();

        // WSL warning
        if self.is_wsl {
            warnings.push(EnvironmentWarning {
                code: "WSL_DETECTED",
                message: "Running in WSL2 environment. Some kernel features may be limited. \
                         The Microsoft WSL kernel does not support all audit subsystem features. \
                         This is fine for development, but production deployments should use \
                         native Linux."
                    .to_string(),
                severity: WarningSeverity::Warning,
            });
        }

        // Container without host access warning
        if self.environment == DeploymentEnvironment::ContainerIsolated {
            warnings.push(EnvironmentWarning {
                code: "NO_HOST_ACCESS",
                message: "Running in container without host filesystem access. \
                         File monitoring will only work for container-local files. \
                         For production Kubernetes deployment, mount the host filesystem."
                    .to_string(),
                severity: WarningSeverity::Warning,
            });
        }

        // Audit netlink warning for containerized environments
        if self.environment != DeploymentEnvironment::Host {
            warnings.push(EnvironmentWarning {
                code: "AUDIT_LIMITED",
                message: "Linux audit rules cannot be added for /proc/<pid>/root/* paths. \
                         This is a kernel limitation in containerized environments. \
                         For process attribution, use eBPF (kernel 5.7+ with BTF). \
                         See daemons/common/EBPF.md for setup instructions."
                    .to_string(),
                severity: WarningSeverity::Info,
            });
        }

        warnings
    }

    fn validate_for_feature(&self, feature: Feature) -> Result<(), EnvironmentError> {
        match feature {
            Feature::Inotify => {
                // inotify is available in all Linux environments
                Ok(())
            }

            Feature::Fanotify => {
                // fanotify requires CAP_SYS_ADMIN, checked separately by CapabilityChecker
                Ok(())
            }

            Feature::FanotifyPermission => {
                // Permission events need specific kernel support
                // This is a basic check - full validation happens at runtime
                Ok(())
            }

            Feature::AuditNetlink => {
                // Audit netlink does NOT work properly in containers for /proc paths
                if self.environment != DeploymentEnvironment::Host {
                    return Err(EnvironmentError::FeatureUnavailable {
                        feature,
                        reason: "Linux audit rules cannot watch /proc/<pid>/root/* paths in containers. \
                                For process attribution in containers, use eBPF with the 'ebpf' feature \
                                (requires kernel 5.7+ with CONFIG_BPF_LSM=y and BTF). \
                                See daemons/common/EBPF.md for details."
                            .to_string(),
                    });
                }

                // WSL also has limited audit support
                if self.is_wsl {
                    return Err(EnvironmentError::FeatureUnavailable {
                        feature,
                        reason: "WSL2 kernel has limited audit subsystem support. \
                                The NETLINK_AUDIT socket may not be available."
                            .to_string(),
                    });
                }

                Ok(())
            }

            Feature::ProcAccess => {
                // /proc is available in all Linux environments
                if !Path::new("/proc").exists() {
                    return Err(EnvironmentError::FeatureUnavailable {
                        feature,
                        reason: "/proc filesystem not available".to_string(),
                    });
                }
                Ok(())
            }

            Feature::Ebpf => {
                // Check kernel version >= 4.4 for basic eBPF support
                if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
                    let parts: Vec<&str> = release.trim().split('.').collect();
                    if parts.len() >= 2 {
                        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                            if major < 4 || (major == 4 && minor < 4) {
                                return Err(EnvironmentError::FeatureUnavailable {
                                    feature,
                                    reason: format!(
                                        "Kernel {}.{} does not support eBPF. Requires 4.4+",
                                        major, minor
                                    ),
                                });
                            }
                        }
                    }
                }
                Ok(())
            }

            Feature::EbpfLsm => {
                // eBPF LSM requires kernel 5.7+ with CONFIG_BPF_LSM=y
                if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
                    let parts: Vec<&str> = release.trim().split('.').collect();
                    if parts.len() >= 2 {
                        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                            if major < 5 || (major == 5 && minor < 7) {
                                return Err(EnvironmentError::FeatureUnavailable {
                                    feature,
                                    reason: format!(
                                        "Kernel {}.{} does not support eBPF LSM. Requires 5.7+",
                                        major, minor
                                    ),
                                });
                            }
                        }
                    }
                }

                // Check for BTF support (required for LSM BPF)
                if !Path::new("/sys/kernel/btf/vmlinux").exists() {
                    return Err(EnvironmentError::FeatureUnavailable {
                        feature,
                        reason: "BTF (BPF Type Format) not available. Kernel must be compiled with CONFIG_DEBUG_INFO_BTF=y".to_string(),
                    });
                }

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_environment_display() {
        assert_eq!(DeploymentEnvironment::Host.to_string(), "Host");
        assert_eq!(
            DeploymentEnvironment::ContainerWithHostAccess.to_string(),
            "Container (with host access)"
        );
        assert_eq!(
            DeploymentEnvironment::ContainerIsolated.to_string(),
            "Container (isolated)"
        );
    }

    #[test]
    fn test_feature_display() {
        assert_eq!(Feature::Inotify.to_string(), "inotify");
        assert_eq!(Feature::Fanotify.to_string(), "fanotify");
        assert_eq!(Feature::AuditNetlink.to_string(), "audit netlink");
    }

    #[test]
    fn test_linux_environment_detector_creation() {
        let detector = LinuxEnvironmentDetector::new();
        // Should not panic
        let _ = detector.detect();
        let _ = detector.can_access_host();
        let _ = detector.environment_warnings();
    }

    #[test]
    fn test_validate_inotify() {
        let detector = LinuxEnvironmentDetector::new();
        // inotify should always be available on Linux
        assert!(detector.validate_for_feature(Feature::Inotify).is_ok());
    }

    #[test]
    fn test_validate_proc_access() {
        let detector = LinuxEnvironmentDetector::new();
        // /proc should be available on Linux
        assert!(detector.validate_for_feature(Feature::ProcAccess).is_ok());
    }

    #[test]
    fn test_warning_severity_display() {
        assert_eq!(WarningSeverity::Info.to_string(), "INFO");
        assert_eq!(WarningSeverity::Warning.to_string(), "WARN");
        assert_eq!(WarningSeverity::Error.to_string(), "ERROR");
    }
}
