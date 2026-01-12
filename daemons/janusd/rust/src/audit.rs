// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Kernel Audit Integration
//!
//! This module provides integration with the Linux kernel audit subsystem
//! via netlink sockets (NETLINK_AUDIT).
//!
//! ## Netlink Interface
//!
//! The audit subsystem is accessed via `AF_NETLINK` sockets with protocol
//! `NETLINK_AUDIT`. Messages are formatted as `struct nlmsghdr` followed
//! by audit-specific data.
//!
//! ## Audit Message Types
//!
//! | Type | Value | Description |
//! |------|-------|-------------|
//! | `AUDIT_USER` | 1005 | User-space audit message |
//! | `AUDIT_USER_AVC` | 1107 | User-space AVC message |
//! | `AUDIT_USER_TTY` | 1124 | User TTY input |
//!
//! ## Message Format
//!
//! Audit messages use key=value format:
//! ```text
//! op=janus type=DENIED guard="guard_name" pod="pod_name"
//! path="/etc/passwd" pid=1234 uid=1000 allowed=no
//! ```
//!
//! ## Capabilities Required
//!
//! - `CAP_AUDIT_WRITE` - Write records to kernel audit log
//!
//! ## References
//!
//! - `man 7 netlink` - Netlink socket interface
//! - `man 3 audit_open` - Audit library functions
//! - Linux kernel source: `kernel/audit.c`

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicU32, Ordering};

use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::guard::AccessResponse;

/// Netlink protocol for kernel audit.
const NETLINK_AUDIT: i32 = 9;

/// Audit message type for user-space messages.
const AUDIT_USER: u16 = 1005;

/// Netlink message header flags.
const NLM_F_REQUEST: u16 = 1;
const NLM_F_ACK: u16 = 4;

/// Netlink message header size.
const NLMSG_HDRLEN: usize = 16;

/// Errors from the audit module.
#[derive(Error, Debug)]
pub enum AuditError {
    #[error("failed to create netlink socket: {0}")]
    SocketCreate(io::Error),

    #[error("failed to bind netlink socket: {0}")]
    SocketBind(io::Error),

    #[error("failed to send audit message: {0}")]
    SendMessage(io::Error),

    #[error("missing CAP_AUDIT_WRITE capability")]
    MissingCapability,

    #[error("audit message too long: {0} bytes (max 8192)")]
    MessageTooLong(usize),
}

/// Audit event type for logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventType {
    /// Access event (file read/write)
    Access,
    /// Open event (file opened)
    Open,
    /// Access was denied
    Denied,
    /// Policy change
    Policy,
}

impl AuditEventType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "ACCESS",
            Self::Open => "OPEN",
            Self::Denied => "DENIED",
            Self::Policy => "POLICY",
        }
    }
}

/// Access event information for audit logging.
#[derive(Debug, Clone)]
pub struct AuditAccessEvent {
    pub guard_name: String,
    pub namespace: String,
    pub pod_name: String,
    pub container_id: String,
    pub path: String,
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
    pub response: AccessResponse,
    pub event_type: AuditEventType,
}

impl AuditAccessEvent {
    /// Format the event as an audit message string.
    pub fn to_audit_message(&self) -> String {
        let allowed = match self.response {
            AccessResponse::Allow | AccessResponse::Audit => "yes",
            AccessResponse::Deny => "no",
        };

        format!(
            "op=janus type={} guard=\"{}\" namespace=\"{}\" pod=\"{}\" \
             container=\"{}\" path=\"{}\" pid={} uid={} gid={} allowed={}",
            self.event_type.as_str(),
            self.guard_name,
            self.namespace,
            self.pod_name,
            self.container_id,
            self.path,
            self.pid,
            self.uid,
            self.gid,
            allowed
        )
    }
}

/// Policy change information for audit logging.
#[derive(Debug, Clone)]
pub struct AuditPolicyChange {
    pub guard_name: String,
    pub namespace: String,
    pub action: String, // "add", "remove", "update"
    pub patterns_added: usize,
    pub patterns_removed: usize,
}

impl AuditPolicyChange {
    /// Format the policy change as an audit message string.
    pub fn to_audit_message(&self) -> String {
        format!(
            "op=janus type=POLICY guard=\"{}\" namespace=\"{}\" \
             action=\"{}\" patterns_added={} patterns_removed={}",
            self.guard_name,
            self.namespace,
            self.action,
            self.patterns_added,
            self.patterns_removed
        )
    }
}

/// Trait for audit logging implementations.
pub trait AuditLogger: Send + Sync {
    /// Log an access event to the kernel audit log.
    fn log_access(&self, event: &AuditAccessEvent) -> Result<(), AuditError>;

    /// Log a policy change to the kernel audit log.
    fn log_policy_change(&self, change: &AuditPolicyChange) -> Result<(), AuditError>;

    /// Check if audit logging is available.
    fn is_available(&self) -> bool;
}

/// Netlink-based audit logger that writes to the kernel audit subsystem.
pub struct NetlinkAuditLogger {
    socket_fd: RawFd,
    sequence: AtomicU32,
}

impl NetlinkAuditLogger {
    /// Create a new netlink audit logger.
    ///
    /// # Errors
    ///
    /// Returns `AuditError::SocketCreate` if the netlink socket cannot be created.
    /// Returns `AuditError::SocketBind` if the socket cannot be bound.
    /// Returns `AuditError::MissingCapability` if CAP_AUDIT_WRITE is not available.
    pub fn new() -> Result<Self, AuditError> {
        // Check for CAP_AUDIT_WRITE capability
        if let Ok(caps) = caps::read(None, caps::CapSet::Effective) {
            if !caps.contains(&caps::Capability::CAP_AUDIT_WRITE) {
                warn!("CAP_AUDIT_WRITE not available, audit logging will be disabled");
                return Err(AuditError::MissingCapability);
            }
        }

        // Create netlink socket
        let socket_fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                NETLINK_AUDIT,
            )
        };

        if socket_fd < 0 {
            return Err(AuditError::SocketCreate(io::Error::last_os_error()));
        }

        // Bind the socket
        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_pid = 0; // Let kernel assign
        addr.nl_groups = 0;

        let ret = unsafe {
            libc::bind(
                socket_fd,
                &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };

        if ret < 0 {
            unsafe { libc::close(socket_fd) };
            return Err(AuditError::SocketBind(io::Error::last_os_error()));
        }

        info!("Netlink audit logger initialized");

        Ok(Self {
            socket_fd,
            sequence: AtomicU32::new(1),
        })
    }

    /// Send an audit message to the kernel.
    fn send_message(&self, message: &str) -> Result<(), AuditError> {
        let msg_bytes = message.as_bytes();

        // Check message length (max 8192 bytes for audit messages)
        if msg_bytes.len() > 8192 {
            return Err(AuditError::MessageTooLong(msg_bytes.len()));
        }

        // Build netlink message
        let total_len = NLMSG_HDRLEN + msg_bytes.len();
        let aligned_len = (total_len + 3) & !3; // Align to 4 bytes

        let mut buffer = vec![0u8; aligned_len];

        // Fill in netlink header
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);

        // nlmsg_len (u32)
        buffer[0..4].copy_from_slice(&(total_len as u32).to_ne_bytes());
        // nlmsg_type (u16)
        buffer[4..6].copy_from_slice(&AUDIT_USER.to_ne_bytes());
        // nlmsg_flags (u16)
        buffer[6..8].copy_from_slice(&(NLM_F_REQUEST | NLM_F_ACK).to_ne_bytes());
        // nlmsg_seq (u32)
        buffer[8..12].copy_from_slice(&seq.to_ne_bytes());
        // nlmsg_pid (u32) - our PID
        buffer[12..16].copy_from_slice(&(std::process::id() as u32).to_ne_bytes());

        // Copy message payload
        buffer[NLMSG_HDRLEN..NLMSG_HDRLEN + msg_bytes.len()].copy_from_slice(msg_bytes);

        // Send to kernel
        let mut dest_addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        dest_addr.nl_family = libc::AF_NETLINK as u16;
        dest_addr.nl_pid = 0; // Kernel
        dest_addr.nl_groups = 0;

        let ret = unsafe {
            libc::sendto(
                self.socket_fd,
                buffer.as_ptr() as *const libc::c_void,
                buffer.len(),
                0,
                &dest_addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };

        if ret < 0 {
            return Err(AuditError::SendMessage(io::Error::last_os_error()));
        }

        debug!(seq = seq, len = msg_bytes.len(), "Sent audit message");

        Ok(())
    }
}

impl Drop for NetlinkAuditLogger {
    fn drop(&mut self) {
        unsafe { libc::close(self.socket_fd) };
    }
}

impl AuditLogger for NetlinkAuditLogger {
    fn log_access(&self, event: &AuditAccessEvent) -> Result<(), AuditError> {
        let message = event.to_audit_message();
        self.send_message(&message)
    }

    fn log_policy_change(&self, change: &AuditPolicyChange) -> Result<(), AuditError> {
        let message = change.to_audit_message();
        self.send_message(&message)
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl AsRawFd for NetlinkAuditLogger {
    fn as_raw_fd(&self) -> RawFd {
        self.socket_fd
    }
}

/// A no-op audit logger for when audit is disabled or unavailable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAuditLogger;

impl AuditLogger for NullAuditLogger {
    fn log_access(&self, _event: &AuditAccessEvent) -> Result<(), AuditError> {
        Ok(())
    }

    fn log_policy_change(&self, _change: &AuditPolicyChange) -> Result<(), AuditError> {
        Ok(())
    }

    fn is_available(&self) -> bool {
        false
    }
}

/// Create an audit logger, falling back to null logger if unavailable.
pub fn create_audit_logger() -> Box<dyn AuditLogger> {
    match NetlinkAuditLogger::new() {
        Ok(logger) => Box::new(logger),
        Err(e) => {
            warn!(error = %e, "Failed to create audit logger, using null logger");
            Box::new(NullAuditLogger)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_type_as_str() {
        assert_eq!(AuditEventType::Access.as_str(), "ACCESS");
        assert_eq!(AuditEventType::Open.as_str(), "OPEN");
        assert_eq!(AuditEventType::Denied.as_str(), "DENIED");
        assert_eq!(AuditEventType::Policy.as_str(), "POLICY");
    }

    #[test]
    fn test_audit_access_event_to_message() {
        let event = AuditAccessEvent {
            guard_name: "my-guard".to_string(),
            namespace: "default".to_string(),
            pod_name: "pod-abc".to_string(),
            container_id: "container-123".to_string(),
            path: "/etc/passwd".to_string(),
            pid: 1234,
            uid: 1000,
            gid: 1000,
            response: AccessResponse::Deny,
            event_type: AuditEventType::Denied,
        };

        let message = event.to_audit_message();

        assert!(message.contains("op=janus"));
        assert!(message.contains("type=DENIED"));
        assert!(message.contains("guard=\"my-guard\""));
        assert!(message.contains("namespace=\"default\""));
        assert!(message.contains("pod=\"pod-abc\""));
        assert!(message.contains("path=\"/etc/passwd\""));
        assert!(message.contains("pid=1234"));
        assert!(message.contains("uid=1000"));
        assert!(message.contains("allowed=no"));
    }

    #[test]
    fn test_audit_access_event_allowed() {
        let event = AuditAccessEvent {
            guard_name: "guard".to_string(),
            namespace: "ns".to_string(),
            pod_name: "pod".to_string(),
            container_id: "ctr".to_string(),
            path: "/file".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            response: AccessResponse::Allow,
            event_type: AuditEventType::Access,
        };

        let message = event.to_audit_message();
        assert!(message.contains("allowed=yes"));
    }

    #[test]
    fn test_audit_access_event_audit_response() {
        let event = AuditAccessEvent {
            guard_name: "guard".to_string(),
            namespace: "ns".to_string(),
            pod_name: "pod".to_string(),
            container_id: "ctr".to_string(),
            path: "/file".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            response: AccessResponse::Audit,
            event_type: AuditEventType::Open,
        };

        let message = event.to_audit_message();
        assert!(message.contains("type=OPEN"));
        assert!(message.contains("allowed=yes"));
    }

    #[test]
    fn test_audit_policy_change_to_message() {
        let change = AuditPolicyChange {
            guard_name: "my-guard".to_string(),
            namespace: "default".to_string(),
            action: "update".to_string(),
            patterns_added: 5,
            patterns_removed: 2,
        };

        let message = change.to_audit_message();

        assert!(message.contains("op=janus"));
        assert!(message.contains("type=POLICY"));
        assert!(message.contains("guard=\"my-guard\""));
        assert!(message.contains("action=\"update\""));
        assert!(message.contains("patterns_added=5"));
        assert!(message.contains("patterns_removed=2"));
    }

    #[test]
    fn test_null_audit_logger() {
        let logger = NullAuditLogger;

        assert!(!logger.is_available());

        let event = AuditAccessEvent {
            guard_name: "guard".to_string(),
            namespace: "ns".to_string(),
            pod_name: "pod".to_string(),
            container_id: "ctr".to_string(),
            path: "/file".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            response: AccessResponse::Allow,
            event_type: AuditEventType::Access,
        };

        assert!(logger.log_access(&event).is_ok());

        let change = AuditPolicyChange {
            guard_name: "guard".to_string(),
            namespace: "ns".to_string(),
            action: "add".to_string(),
            patterns_added: 1,
            patterns_removed: 0,
        };

        assert!(logger.log_policy_change(&change).is_ok());
    }

    #[test]
    fn test_create_audit_logger_fallback() {
        // This will likely fall back to null logger in test environment
        // (no CAP_AUDIT_WRITE)
        let logger = create_audit_logger();

        // Should at least be callable
        let event = AuditAccessEvent {
            guard_name: "test".to_string(),
            namespace: "test".to_string(),
            pod_name: "test".to_string(),
            container_id: "test".to_string(),
            path: "/test".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            response: AccessResponse::Allow,
            event_type: AuditEventType::Access,
        };

        // Should not panic
        let _ = logger.log_access(&event);
    }

    #[test]
    fn test_audit_message_special_chars() {
        let event = AuditAccessEvent {
            guard_name: "guard-with-special".to_string(),
            namespace: "kube-system".to_string(),
            pod_name: "pod_with_underscore".to_string(),
            container_id: "abc123def".to_string(),
            path: "/path/with spaces/and\"quotes".to_string(),
            pid: 9999,
            uid: 65534,
            gid: 65534,
            response: AccessResponse::Deny,
            event_type: AuditEventType::Denied,
        };

        let message = event.to_audit_message();

        // Should contain the path with special chars
        assert!(message.contains("/path/with spaces/and\"quotes"));
    }
}
