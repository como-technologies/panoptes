//! # Janus eBPF Event Processing
//!
//! Converts raw eBPF events to Janus-specific access audit events.
//! This module handles the conversion from shared `FileEvent` types to
//! Janus protobuf types.

use panoptes_common::ebpf::{FileEvent, FileEventType};

/// Access event from eBPF with full process attribution.
///
/// This is a Janus-specific wrapper around the shared `FileEvent` type
/// that provides methods for converting to Janus protobuf types.
///
/// ## Process Attribution
///
/// Unlike fanotify (which only provides PID), eBPF captures:
/// - `pid` / `tgid` - Process and thread group ID
/// - `uid` / `gid` - User and group ID
/// - `comm` - Process command name (up to 16 chars)
///
/// All captured atomically at syscall time, solving the /proc TOCTOU race.
#[derive(Debug, Clone)]
pub struct EbpfAccessEvent {
    /// Event type (access, open_read, open_write)
    pub event_type: FileEventType,
    /// Path to the file
    pub path: String,
    /// Process ID that triggered the event
    pub pid: u32,
    /// Thread group ID
    pub tgid: u32,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Process command name
    pub comm: String,
    /// Kernel timestamp in nanoseconds
    pub timestamp_ns: u64,
}

impl From<FileEvent> for EbpfAccessEvent {
    fn from(event: FileEvent) -> Self {
        let event_type = FileEventType::from_u32(event.event_type).unwrap_or(FileEventType::Access);

        Self {
            event_type,
            path: event.path_str().to_string(),
            pid: event.pid,
            tgid: event.tgid,
            uid: event.uid,
            gid: event.gid,
            comm: event.comm_str().to_string(),
            timestamp_ns: event.timestamp_ns,
        }
    }
}

impl EbpfAccessEvent {
    /// Get the event type as a string
    pub fn event_type_str(&self) -> &str {
        self.event_type.as_str()
    }

    /// Check if this event has process information
    pub fn has_process_info(&self) -> bool {
        self.pid > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebpf_access_event_from_raw() {
        let mut raw = FileEvent::default();
        raw.event_type = FileEventType::Access as u32;
        raw.pid = 1234;
        raw.uid = 1000;

        // Set path
        let path = b"/etc/passwd";
        raw.path[..path.len()].copy_from_slice(path);

        // Set comm
        let comm = b"cat";
        raw.comm[..comm.len()].copy_from_slice(comm);

        let event = EbpfAccessEvent::from(raw);

        assert_eq!(event.event_type, FileEventType::Access);
        assert_eq!(event.pid, 1234);
        assert_eq!(event.uid, 1000);
        assert_eq!(event.path, "/etc/passwd");
        assert_eq!(event.comm, "cat");
        assert!(event.has_process_info());
    }

    #[test]
    fn test_event_type_str() {
        let event = EbpfAccessEvent {
            event_type: FileEventType::OpenRead,
            path: String::new(),
            pid: 0,
            tgid: 0,
            uid: 0,
            gid: 0,
            comm: String::new(),
            timestamp_ns: 0,
        };
        assert_eq!(event.event_type_str(), "open_read");
        assert!(!event.has_process_info()); // pid is 0
    }
}
