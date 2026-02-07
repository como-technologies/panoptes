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
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicU32, Ordering};

use nix::sys::socket::{bind, sendto, MsgFlags, NetlinkAddr};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Access response type for audit events.
///
/// Defined here (not in guard module) so it's available in both
/// traditional (fanotify) and eBPF modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessResponse {
    /// Access was allowed
    Allow,
    /// Access was denied
    Deny,
    /// Access was audited (logged without policy enforcement)
    Audit,
}

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
}

impl AuditEventType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "ACCESS",
            Self::Open => "OPEN",
            Self::Denied => "DENIED",
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
    /// Process name (comm) from /proc/{pid}/stat
    pub comm: String,
    /// Executable path from /proc/{pid}/exe
    pub exe: String,
}

impl AuditAccessEvent {
    /// Format the event as an audit message string.
    ///
    /// For compliance (PCI-DSS, HIPAA, SOC2), audit messages must answer:
    /// - WHO: uid, gid, comm (process name), exe (executable path)
    /// - WHAT: type (ACCESS/OPEN/DENIED), path, allowed
    /// - WHEN: implicit in kernel audit timestamp
    /// - WHERE: guard, namespace, pod, container
    pub fn to_audit_message(&self) -> String {
        let allowed = match self.response {
            AccessResponse::Allow | AccessResponse::Audit => "yes",
            AccessResponse::Deny => "no",
        };

        // Base format with all required compliance fields
        let mut msg = format!(
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
        );

        // Add process attribution if available (WHO made the access)
        if !self.comm.is_empty() {
            msg.push_str(&format!(" comm=\"{}\"", self.comm));
        }
        if !self.exe.is_empty() {
            msg.push_str(&format!(" exe=\"{}\"", self.exe));
        }

        msg
    }
}

/// Trait for audit logging implementations.
pub trait AuditLogger: Send + Sync {
    /// Log an access event to the kernel audit log.
    fn log_access(&self, event: &AuditAccessEvent) -> Result<(), AuditError>;

    /// Check if audit logging is available.
    fn is_available(&self) -> bool;
}

/// Netlink-based audit logger that writes to the kernel audit subsystem.
///
/// Uses `OwnedFd` for the socket, which provides automatic cleanup via `Drop`.
/// This eliminates the need for a manual `Drop` implementation and reduces
/// the risk of file descriptor leaks.
pub struct NetlinkAuditLogger {
    /// Owned file descriptor for the netlink socket.
    /// Automatically closed when the logger is dropped.
    socket_fd: OwnedFd,
    /// Sequence number for netlink messages.
    sequence: AtomicU32,
}

impl NetlinkAuditLogger {
    /// Create a new netlink audit logger.
    ///
    /// # Unsafe Code Minimization
    ///
    /// The only `unsafe` block is for `libc::socket()` because nix's `socket()`
    /// function requires a `SockProtocol` enum, and `NETLINK_AUDIT` (protocol 9)
    /// is not exposed in that enum. The raw fd is immediately wrapped in `OwnedFd`
    /// for automatic cleanup.
    ///
    /// All other operations use nix's safe wrappers:
    /// - `NetlinkAddr` instead of `sockaddr_nl` with `mem::zeroed()`
    /// - `nix::sys::socket::bind()` instead of `libc::bind()`
    /// - `OwnedFd::drop()` handles close automatically
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

        // Create netlink socket.
        //
        // SAFETY: libc::socket() is safe to call. We immediately check the return
        // value and convert to OwnedFd for automatic cleanup. The only reason we
        // use libc directly is that nix's SockProtocol enum doesn't include
        // NETLINK_AUDIT (protocol 9).
        //
        // If socket() fails, it returns -1 and sets errno. We convert that to a
        // Rust io::Error. If it succeeds, we immediately wrap the fd in OwnedFd
        // so any subsequent errors will still clean up the socket.
        let raw_fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                NETLINK_AUDIT,
            )
        };

        if raw_fd < 0 {
            return Err(AuditError::SocketCreate(io::Error::last_os_error()));
        }

        // SAFETY: We just verified raw_fd is a valid fd from socket().
        // OwnedFd takes ownership and will close it when dropped.
        let socket_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

        // Bind the socket using nix's safe wrapper.
        // NetlinkAddr::new(pid, groups) creates a properly initialized address:
        // - pid=0 lets kernel assign a port ID
        // - groups=0 means no multicast group subscriptions
        let addr = NetlinkAddr::new(0, 0);

        bind(socket_fd.as_raw_fd(), &addr).map_err(|e| {
            // OwnedFd will close the socket automatically when dropped
            AuditError::SocketBind(io::Error::from_raw_os_error(e as i32))
        })?;

        info!("Netlink audit logger initialized");

        Ok(Self {
            socket_fd,
            sequence: AtomicU32::new(1),
        })
    }

    /// Send an audit message to the kernel.
    ///
    /// Uses nix's safe `sendto()` wrapper instead of raw libc calls.
    /// The netlink message header is built manually (no unsafe needed for that),
    /// then sent using nix's type-safe socket operations.
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

        // Fill in netlink header (struct nlmsghdr)
        // This is pure data manipulation, no unsafe needed.
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
        buffer[12..16].copy_from_slice(&std::process::id().to_ne_bytes());

        // Copy message payload
        buffer[NLMSG_HDRLEN..NLMSG_HDRLEN + msg_bytes.len()].copy_from_slice(msg_bytes);

        // Send to kernel using nix's safe sendto() wrapper.
        // NetlinkAddr::new(0, 0) creates the destination address:
        // - pid=0 means send to kernel
        // - groups=0 means no multicast
        let dest_addr = NetlinkAddr::new(0, 0);

        sendto(
            self.socket_fd.as_raw_fd(),
            &buffer,
            &dest_addr,
            MsgFlags::empty(),
        )
        .map_err(|e| AuditError::SendMessage(io::Error::from_raw_os_error(e as i32)))?;

        debug!(seq = seq, len = msg_bytes.len(), "Sent audit message");

        Ok(())
    }
}

// Note: No manual Drop implementation needed.
// OwnedFd automatically closes the socket when NetlinkAuditLogger is dropped.
// This is safer than manual libc::close() calls:
// - No risk of double-close
// - No risk of use-after-close
// - No unsafe code needed for cleanup

impl AuditLogger for NetlinkAuditLogger {
    fn log_access(&self, event: &AuditAccessEvent) -> Result<(), AuditError> {
        let message = event.to_audit_message();
        self.send_message(&message)
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl AsRawFd for NetlinkAuditLogger {
    fn as_raw_fd(&self) -> RawFd {
        self.socket_fd.as_raw_fd()
    }
}

/// A no-op audit logger for when audit is disabled or unavailable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAuditLogger;

impl AuditLogger for NullAuditLogger {
    fn log_access(&self, _event: &AuditAccessEvent) -> Result<(), AuditError> {
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
            comm: "cat".to_string(),
            exe: "/usr/bin/cat".to_string(),
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
        // Process attribution fields for compliance
        assert!(message.contains("comm=\"cat\""));
        assert!(message.contains("exe=\"/usr/bin/cat\""));
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
            comm: String::new(),
            exe: String::new(),
        };

        let message = event.to_audit_message();
        assert!(message.contains("allowed=yes"));
        // Empty comm/exe should not be included in message
        assert!(!message.contains("comm="));
        assert!(!message.contains("exe="));
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
            comm: String::new(),
            exe: String::new(),
        };

        let message = event.to_audit_message();
        assert!(message.contains("type=OPEN"));
        assert!(message.contains("allowed=yes"));
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
            comm: String::new(),
            exe: String::new(),
        };

        assert!(logger.log_access(&event).is_ok());
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
            comm: String::new(),
            exe: String::new(),
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
            comm: "special-proc".to_string(),
            exe: "/usr/local/bin/special-proc".to_string(),
        };

        let message = event.to_audit_message();

        // Should contain the path with special chars
        assert!(message.contains("/path/with spaces/and\"quotes"));
        assert!(message.contains("comm=\"special-proc\""));
    }
}
