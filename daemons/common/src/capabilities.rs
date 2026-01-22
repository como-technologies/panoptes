//! # Linux Capability Verification
//!
//! This module provides traits and implementations for verifying Linux capabilities
//! required by Panoptes daemons.
//!
//! ## Design Philosophy
//!
//! Rather than letting daemons fail mysteriously when operations require capabilities
//! they don't have, we verify capabilities at startup and fail fast with clear error
//! messages explaining what's missing and why it's needed.
//!
//! ## Required Capabilities by Daemon
//!
//! | Daemon | Capability | Purpose |
//! |--------|------------|---------|
//! | argusd | CAP_SYS_PTRACE | Access `/proc/<pid>/root` for container filesystems |
//! | janusd | CAP_SYS_ADMIN | fanotify operations |
//! | janusd | CAP_SYS_PTRACE | Access `/proc/<pid>/root` for container filesystems |
//! | janusd | CAP_AUDIT_WRITE | Write to kernel audit log (optional) |
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::capabilities::{
//!     LinuxCapabilityChecker, CapabilityChecker, RequiredCapability,
//!     ARGUSD_REQUIRED_CAPS,
//! };
//!
//! let checker = LinuxCapabilityChecker::new();
//!
//! // Check all required capabilities at startup
//! let missing = checker.check_required(ARGUSD_REQUIRED_CAPS);
//! if !missing.is_empty() {
//!     for cap in &missing {
//!         eprintln!("Missing capability: {} - {}", cap, cap.description());
//!     }
//!     std::process::exit(1);
//! }
//! ```

use std::fmt;
use thiserror::Error;

/// Linux capabilities required by Panoptes daemons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredCapability {
    /// CAP_SYS_ADMIN - Required for fanotify operations.
    SysAdmin,

    /// CAP_SYS_PTRACE - Required for accessing `/proc/<pid>/root`.
    SysPtrace,

    /// CAP_DAC_READ_SEARCH - Bypass file read permission checks.
    DacReadSearch,

    /// CAP_AUDIT_WRITE - Write records to kernel audit log.
    AuditWrite,

    /// CAP_AUDIT_READ - Read audit log via netlink.
    AuditRead,

    /// CAP_AUDIT_CONTROL - Configure audit subsystem.
    AuditControl,

    /// CAP_BPF - Load and run eBPF programs (Linux 5.8+).
    Bpf,

    /// CAP_PERFMON - Attach eBPF to tracepoints/kprobes (Linux 5.8+).
    Perfmon,
}

impl RequiredCapability {
    /// Get the Linux capability constant for this capability.
    pub fn to_caps_capability(&self) -> caps::Capability {
        match self {
            Self::SysAdmin => caps::Capability::CAP_SYS_ADMIN,
            Self::SysPtrace => caps::Capability::CAP_SYS_PTRACE,
            Self::DacReadSearch => caps::Capability::CAP_DAC_READ_SEARCH,
            Self::AuditWrite => caps::Capability::CAP_AUDIT_WRITE,
            Self::AuditRead => caps::Capability::CAP_AUDIT_READ,
            Self::AuditControl => caps::Capability::CAP_AUDIT_CONTROL,
            Self::Bpf => caps::Capability::CAP_BPF,
            Self::Perfmon => caps::Capability::CAP_PERFMON,
        }
    }

    /// Get a human-readable description of what this capability is needed for.
    pub fn description(&self) -> &'static str {
        match self {
            Self::SysAdmin => "Required for fanotify file access monitoring",
            Self::SysPtrace => "Required to access container filesystems via /proc/<pid>/root",
            Self::DacReadSearch => {
                "Bypass file permission checks (optional, enables broader access)"
            }
            Self::AuditWrite => "Write events to kernel audit log",
            Self::AuditRead => "Read events from kernel audit subsystem",
            Self::AuditControl => "Configure kernel audit rules",
            Self::Bpf => "Load and run eBPF programs for kernel-level monitoring",
            Self::Perfmon => "Attach eBPF programs to tracepoints and kprobes",
        }
    }

    /// Get the capability name as used in Kubernetes securityContext.
    pub fn k8s_name(&self) -> &'static str {
        match self {
            Self::SysAdmin => "SYS_ADMIN",
            Self::SysPtrace => "SYS_PTRACE",
            Self::DacReadSearch => "DAC_READ_SEARCH",
            Self::AuditWrite => "AUDIT_WRITE",
            Self::AuditRead => "AUDIT_READ",
            Self::AuditControl => "AUDIT_CONTROL",
            Self::Bpf => "BPF",
            Self::Perfmon => "PERFMON",
        }
    }
}

impl fmt::Display for RequiredCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CAP_{}", self.k8s_name())
    }
}

/// Capabilities required by argusd (file integrity monitoring) - inotify mode.
pub const ARGUSD_REQUIRED_CAPS: &[RequiredCapability] = &[RequiredCapability::SysPtrace];

/// Capabilities required by argusd with eBPF-based file monitoring.
/// Note: CAP_SYS_ADMIN can be used instead of CAP_BPF + CAP_PERFMON on older kernels.
pub const ARGUSD_REQUIRED_CAPS_EBPF: &[RequiredCapability] = &[
    RequiredCapability::SysPtrace,
    RequiredCapability::Bpf,
    RequiredCapability::Perfmon,
];

/// Capabilities required by janusd (file access auditing).
pub const JANUSD_REQUIRED_CAPS: &[RequiredCapability] =
    &[RequiredCapability::SysAdmin, RequiredCapability::SysPtrace];

/// Optional capabilities for janusd audit logging.
pub const JANUSD_OPTIONAL_CAPS: &[RequiredCapability] = &[RequiredCapability::AuditWrite];

/// Capabilities required by janusd with eBPF-based file access auditing.
/// Note: CAP_SYS_ADMIN can be used instead of CAP_BPF + CAP_PERFMON on older kernels.
pub const JANUSD_REQUIRED_CAPS_EBPF: &[RequiredCapability] = &[
    RequiredCapability::SysAdmin,
    RequiredCapability::SysPtrace,
    RequiredCapability::Bpf,
    RequiredCapability::Perfmon,
];

/// Errors from capability checking.
#[derive(Debug, Error)]
pub enum CapabilityError {
    /// One or more required capabilities are missing.
    #[error("Missing required capabilities: {}", format_capabilities(.0))]
    MissingCapabilities(Vec<RequiredCapability>),

    /// A specific capability is missing.
    #[error("Missing capability {capability}: {}", capability.description())]
    MissingCapability { capability: RequiredCapability },

    /// Failed to query capabilities.
    #[error("Failed to query capabilities: {0}")]
    QueryFailed(String),
}

/// Format a list of capabilities for display.
fn format_capabilities(caps: &[RequiredCapability]) -> String {
    caps.iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Trait for checking Linux capabilities.
pub trait CapabilityChecker: Send + Sync {
    /// Check if a specific capability is available.
    fn has_capability(&self, cap: RequiredCapability) -> bool;

    /// Require a capability, returning error if missing.
    fn require(&self, cap: RequiredCapability) -> Result<(), CapabilityError>;

    /// Check all required capabilities, returning list of missing ones.
    fn check_required(&self, caps: &[RequiredCapability]) -> Vec<RequiredCapability>;

    /// Require all capabilities in the list, returning error if any are missing.
    fn require_all(&self, caps: &[RequiredCapability]) -> Result<(), CapabilityError> {
        let missing = self.check_required(caps);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CapabilityError::MissingCapabilities(missing))
        }
    }
}

/// Linux-specific capability checker using the `caps` crate.
pub struct LinuxCapabilityChecker {
    /// Cached effective capabilities.
    effective: Option<caps::CapsHashSet>,
}

impl LinuxCapabilityChecker {
    /// Create a new capability checker.
    ///
    /// Queries effective capabilities at creation time.
    pub fn new() -> Self {
        let effective = caps::read(None, caps::CapSet::Effective).ok();
        Self { effective }
    }
}

impl Default for LinuxCapabilityChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityChecker for LinuxCapabilityChecker {
    fn has_capability(&self, cap: RequiredCapability) -> bool {
        match &self.effective {
            Some(caps) => caps.contains(&cap.to_caps_capability()),
            None => false,
        }
    }

    fn require(&self, cap: RequiredCapability) -> Result<(), CapabilityError> {
        if self.has_capability(cap) {
            Ok(())
        } else {
            Err(CapabilityError::MissingCapability { capability: cap })
        }
    }

    fn check_required(&self, caps: &[RequiredCapability]) -> Vec<RequiredCapability> {
        caps.iter()
            .filter(|cap| !self.has_capability(**cap))
            .copied()
            .collect()
    }
}

/// Generate a helpful error message for missing capabilities.
pub fn missing_capabilities_message(missing: &[RequiredCapability], daemon: &str) -> String {
    let mut msg = format!(
        "ERROR: {} requires the following Linux capabilities that are not available:\n\n",
        daemon
    );

    for cap in missing {
        msg.push_str(&format!("  {} - {}\n", cap, cap.description()));
    }

    msg.push_str("\nTo fix this in Kubernetes, add to your DaemonSet spec:\n\n");
    msg.push_str("  securityContext:\n");
    msg.push_str("    capabilities:\n");
    msg.push_str("      add:\n");

    for cap in missing {
        msg.push_str(&format!("        - {}\n", cap.k8s_name()));
    }

    msg.push_str("\nOr run the container with --privileged for testing.\n");

    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_capability_display() {
        assert_eq!(RequiredCapability::SysAdmin.to_string(), "CAP_SYS_ADMIN");
        assert_eq!(RequiredCapability::SysPtrace.to_string(), "CAP_SYS_PTRACE");
        assert_eq!(
            RequiredCapability::AuditWrite.to_string(),
            "CAP_AUDIT_WRITE"
        );
    }

    #[test]
    fn test_k8s_names() {
        assert_eq!(RequiredCapability::SysAdmin.k8s_name(), "SYS_ADMIN");
        assert_eq!(RequiredCapability::SysPtrace.k8s_name(), "SYS_PTRACE");
        assert_eq!(
            RequiredCapability::DacReadSearch.k8s_name(),
            "DAC_READ_SEARCH"
        );
    }

    #[test]
    fn test_descriptions() {
        // Ensure all capabilities have descriptions
        for cap in &[
            RequiredCapability::SysAdmin,
            RequiredCapability::SysPtrace,
            RequiredCapability::DacReadSearch,
            RequiredCapability::AuditWrite,
            RequiredCapability::AuditRead,
            RequiredCapability::AuditControl,
        ] {
            assert!(!cap.description().is_empty());
        }
    }

    #[test]
    fn test_linux_capability_checker_creation() {
        // Should not panic
        let _checker = LinuxCapabilityChecker::new();
    }

    #[test]
    fn test_missing_capabilities_message() {
        let missing = vec![RequiredCapability::SysAdmin, RequiredCapability::SysPtrace];
        let msg = missing_capabilities_message(&missing, "janusd");

        assert!(msg.contains("janusd"));
        assert!(msg.contains("SYS_ADMIN"));
        assert!(msg.contains("SYS_PTRACE"));
        assert!(msg.contains("securityContext"));
    }

    #[test]
    fn test_argusd_caps() {
        assert_eq!(ARGUSD_REQUIRED_CAPS.len(), 1);
        assert!(ARGUSD_REQUIRED_CAPS.contains(&RequiredCapability::SysPtrace));
    }

    #[test]
    fn test_janusd_caps() {
        assert_eq!(JANUSD_REQUIRED_CAPS.len(), 2);
        assert!(JANUSD_REQUIRED_CAPS.contains(&RequiredCapability::SysAdmin));
        assert!(JANUSD_REQUIRED_CAPS.contains(&RequiredCapability::SysPtrace));
    }
}
