//! # Argus eBPF Event Processing
//!
//! Converts raw eBPF events to Argus-specific file integrity events.
//! This module handles the conversion from shared `FileEvent` types to
//! Argus protobuf types.

use panoptes_common::ebpf::{FileEvent, FileEventType};

/// File integrity event from eBPF with process attribution.
///
/// This is an Argus-specific wrapper around the shared `FileEvent` type
/// that provides methods for converting to Argus protobuf types.
#[derive(Debug, Clone)]
pub struct EbpfFileEvent {
    /// Event type (create, modify, delete, rename, attrib)
    pub event_type: FileEventType,
    /// Path to the file
    pub path: String,
    /// Process ID that triggered the event
    pub pid: u32,
    /// Process command name
    pub comm: String,
    /// Kernel timestamp in nanoseconds
    pub timestamp_ns: u64,
}

impl From<FileEvent> for EbpfFileEvent {
    fn from(event: FileEvent) -> Self {
        let event_type =
            FileEventType::from_u32(event.event_type).unwrap_or(FileEventType::Modify);

        Self {
            event_type,
            path: event.path_str().to_string(),
            pid: event.pid,
            comm: event.comm_str().to_string(),
            timestamp_ns: event.timestamp_ns,
        }
    }
}

impl EbpfFileEvent {
    /// Get the event type as a string
    pub fn event_type_str(&self) -> &str {
        self.event_type.as_str()
    }

    /// Convert to InotifyEvent enum value for proto compatibility
    pub fn to_inotify_event(&self) -> crate::proto::InotifyEvent {
        use crate::proto::InotifyEvent;
        match self.event_type {
            FileEventType::Create => InotifyEvent::Create,
            FileEventType::Modify => InotifyEvent::Modify,
            FileEventType::Delete => InotifyEvent::Delete,
            FileEventType::Rename => InotifyEvent::MovedTo, // Rename maps to MovedTo
            FileEventType::Attrib => InotifyEvent::Attrib,
            FileEventType::OpenWrite => InotifyEvent::CloseWrite,
            // Access events not typically used in Argus, but handle gracefully
            _ => InotifyEvent::Modify,
        }
    }

    /// Convert to proto FileEvent format (V1)
    pub fn to_proto_v1(
        &self,
        node_name: &str,
        watcher_name: &str,
        namespace: &str,
        pod_name: &str,
        container_id: &str,
    ) -> crate::proto::FileEvent {
        crate::proto::FileEvent {
            timestamp: Some(prost_types::Timestamp {
                seconds: (self.timestamp_ns / 1_000_000_000) as i64,
                nanos: (self.timestamp_ns % 1_000_000_000) as i32,
            }),
            watcher_name: watcher_name.to_string(),
            namespace: namespace.to_string(),
            node_name: node_name.to_string(),
            pod_name: pod_name.to_string(),
            container_id: container_id.to_string(),
            event_type: self.to_inotify_event() as i32,
            path: self.path.clone(),
            filename: self.path.rsplit('/').next().unwrap_or("").to_string(),
            is_directory: false,
            inode: 0,
            tags: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebpf_file_event_from_raw() {
        let mut raw = FileEvent::default();
        raw.event_type = FileEventType::Create as u32;
        raw.pid = 1234;

        // Set path
        let path = b"/etc/passwd";
        raw.path[..path.len()].copy_from_slice(path);

        // Set comm
        let comm = b"vim";
        raw.comm[..comm.len()].copy_from_slice(comm);

        let event = EbpfFileEvent::from(raw);

        assert_eq!(event.event_type, FileEventType::Create);
        assert_eq!(event.pid, 1234);
        assert_eq!(event.path, "/etc/passwd");
        assert_eq!(event.comm, "vim");
    }

    #[test]
    fn test_event_type_mapping() {
        use crate::proto::InotifyEvent;

        let create_event = EbpfFileEvent {
            event_type: FileEventType::Create,
            path: String::new(),
            pid: 0,
            comm: String::new(),
            timestamp_ns: 0,
        };
        assert_eq!(create_event.to_inotify_event(), InotifyEvent::Create);

        let delete_event = EbpfFileEvent {
            event_type: FileEventType::Delete,
            path: String::new(),
            pid: 0,
            comm: String::new(),
            timestamp_ns: 0,
        };
        assert_eq!(delete_event.to_inotify_event(), InotifyEvent::Delete);
    }
}
