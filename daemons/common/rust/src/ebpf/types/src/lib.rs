//! # Panoptes eBPF Shared Types
//!
//! Shared type definitions between eBPF kernel programs and userspace for both
//! Argus (FIM) and Janus (access auditing) daemons.
//!
//! These types must be `#[repr(C)]` for ABI compatibility and use fixed-size
//! arrays since eBPF has no heap allocation.
//!
//! # Usage
//!
//! - **eBPF kernel programs**: Use without features (no_std)
//! - **Userspace**: Use with `user` feature for error handling
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │              eBPF Kernel Programs                       │
//! │  ┌─────────────────┐     ┌─────────────────┐            │
//! │  │ argusd-ebpf     │     │ janusd-ebpf     │            │
//! │  │ (LSM hooks for  │     │ (LSM hooks for  │            │
//! │  │  FIM events)    │     │  access events) │            │
//! │  └────────┬────────┘     └────────┬────────┘            │
//! │           │                       │                     │
//! │           └──────────┬────────────┘                     │
//! │                      │                                  │
//! │               ┌──────▼──────┐                           │
//! │               │ FileEvent   │  (this crate)             │
//! │               │ (shared)    │                           │
//! │               └──────┬──────┘                           │
//! └──────────────────────│──────────────────────────────────┘
//!                        │ Ring Buffer
//! ┌──────────────────────│──────────────────────────────────┐
//! │               Userspace                                 │
//! │               ┌──────▼──────┐                           │
//! │               │ FileEvent   │  (this crate + "user")    │
//! │               └──────┬──────┘                           │
//! │           ┌──────────┴────────────┐                     │
//! │  ┌────────▼────────┐     ┌────────▼────────┐            │
//! │  │ argusd          │     │ janusd          │            │
//! │  │ (→ argus.proto) │     │ (→ janus.proto) │            │
//! │  └─────────────────┘     └─────────────────┘            │
//! └─────────────────────────────────────────────────────────┘
//! ```

#![no_std]

/// Maximum path length in bytes (limited by eBPF stack)
pub const MAX_PATH_LEN: usize = 256;

/// Maximum command name length (TASK_COMM_LEN in kernel)
pub const MAX_COMM_LEN: usize = 16;

/// Maximum container ID length (64 hex chars for SHA256)
pub const MAX_CONTAINER_ID_LEN: usize = 64;

/// File event types shared between Argus (FIM) and Janus (access audit).
///
/// # Argus Events (File Integrity Monitoring)
/// - `Create`, `Modify`, `Delete`, `Rename`, `Attrib` - State changes
/// - `OpenWrite` - Write intent
///
/// # Janus Events (Access Auditing)
/// - `Access` - File read access
/// - `OpenRead` - File opened for reading
/// - `OpenWrite` - File opened for writing (shared with Argus)
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventType {
    /// File created (Argus FIM)
    Create = 1,
    /// File modified/written to (Argus FIM)
    Modify = 2,
    /// File deleted (Argus FIM)
    Delete = 3,
    /// File renamed/moved (Argus FIM)
    Rename = 4,
    /// File opened for write (Argus FIM, Janus audit)
    OpenWrite = 5,
    /// File attribute changed - chmod, chown (Argus FIM)
    Attrib = 6,
    /// File read access (Janus audit)
    Access = 7,
    /// File opened for reading (Janus audit)
    OpenRead = 8,
}

impl FileEventType {
    /// Convert from raw u32 value
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(Self::Create),
            2 => Some(Self::Modify),
            3 => Some(Self::Delete),
            4 => Some(Self::Rename),
            5 => Some(Self::OpenWrite),
            6 => Some(Self::Attrib),
            7 => Some(Self::Access),
            8 => Some(Self::OpenRead),
            _ => None,
        }
    }

    /// Get string representation for the event type
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::OpenWrite => "open_write",
            Self::Attrib => "attrib",
            Self::Access => "access",
            Self::OpenRead => "open_read",
        }
    }

    /// Check if this is a FIM (Argus) event
    pub fn is_fim_event(&self) -> bool {
        matches!(
            self,
            Self::Create | Self::Modify | Self::Delete | Self::Rename | Self::Attrib | Self::OpenWrite
        )
    }

    /// Check if this is an access audit (Janus) event
    pub fn is_access_event(&self) -> bool {
        matches!(self, Self::Access | Self::OpenRead | Self::OpenWrite)
    }
}

/// File event captured by eBPF programs.
///
/// This struct is shared between kernel eBPF programs and userspace.
/// All fields must be fixed-size and `#[repr(C)]` for ABI compatibility.
///
/// # Memory Layout
///
/// The struct is carefully sized to fit within eBPF constraints:
/// - Total size: ~368 bytes
/// - eBPF stack limit: 512 bytes (256 with tail calls)
/// - Ring buffer entry overhead: ~16 bytes
///
/// # Process Attribution
///
/// eBPF captures process context at the kernel level, solving the TOCTOU
/// race condition that occurs when using /proc lookups:
/// - `pid`, `tgid`, `uid`, `gid` - Direct from `bpf_get_current_uid_gid()`
/// - `comm` - From `bpf_get_current_comm()`
/// - `container_id` - Extracted from cgroup path
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileEvent {
    /// Event type (CREATE, MODIFY, DELETE, RENAME, ACCESS, etc.)
    pub event_type: u32,

    /// Process ID that triggered the event
    pub pid: u32,

    /// Thread group ID (for multi-threaded processes)
    pub tgid: u32,

    /// User ID of the process
    pub uid: u32,

    /// Group ID of the process
    pub gid: u32,

    /// Padding for alignment
    pub _pad: u32,

    /// Kernel timestamp in nanoseconds (bpf_ktime_get_ns)
    pub timestamp_ns: u64,

    /// File path (null-terminated, may be truncated)
    pub path: [u8; MAX_PATH_LEN],

    /// Process command name (TASK_COMM_LEN)
    pub comm: [u8; MAX_COMM_LEN],

    /// Container ID extracted from cgroup path (may be empty)
    pub container_id: [u8; MAX_CONTAINER_ID_LEN],
}

impl FileEvent {
    /// Get the path as a string slice (up to first null byte)
    pub fn path_str(&self) -> &str {
        let len = self.path.iter().position(|&b| b == 0).unwrap_or(MAX_PATH_LEN);
        // Safety: We're in no_std, but this is only called from userspace
        // where we have the "user" feature enabled
        core::str::from_utf8(&self.path[..len]).unwrap_or("<invalid utf8>")
    }

    /// Get the command name as a string slice
    pub fn comm_str(&self) -> &str {
        let len = self.comm.iter().position(|&b| b == 0).unwrap_or(MAX_COMM_LEN);
        core::str::from_utf8(&self.comm[..len]).unwrap_or("<invalid utf8>")
    }

    /// Get the container ID as a string slice
    pub fn container_id_str(&self) -> &str {
        let len = self
            .container_id
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(MAX_CONTAINER_ID_LEN);
        core::str::from_utf8(&self.container_id[..len]).unwrap_or("")
    }

    /// Check if this event has a container ID
    pub fn has_container_id(&self) -> bool {
        self.container_id[0] != 0
    }

    /// Get the event type enum
    pub fn event_type(&self) -> Option<FileEventType> {
        FileEventType::from_u32(self.event_type)
    }
}

impl Default for FileEvent {
    fn default() -> Self {
        Self {
            event_type: 0,
            pid: 0,
            tgid: 0,
            uid: 0,
            gid: 0,
            _pad: 0,
            timestamp_ns: 0,
            path: [0; MAX_PATH_LEN],
            comm: [0; MAX_COMM_LEN],
            container_id: [0; MAX_CONTAINER_ID_LEN],
        }
    }
}

// Debug implementation for userspace
impl core::fmt::Debug for FileEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FileEvent")
            .field("event_type", &self.event_type)
            .field("pid", &self.pid)
            .field("tgid", &self.tgid)
            .field("uid", &self.uid)
            .field("gid", &self.gid)
            .field("timestamp_ns", &self.timestamp_ns)
            .field("path", &self.path_str())
            .field("comm", &self.comm_str())
            .field("container_id", &self.container_id_str())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_event_size() {
        // Ensure struct fits in eBPF stack
        assert!(core::mem::size_of::<FileEvent>() < 512);
    }

    #[test]
    fn test_event_type_conversion() {
        assert_eq!(FileEventType::from_u32(1), Some(FileEventType::Create));
        assert_eq!(FileEventType::from_u32(2), Some(FileEventType::Modify));
        assert_eq!(FileEventType::from_u32(7), Some(FileEventType::Access));
        assert_eq!(FileEventType::from_u32(8), Some(FileEventType::OpenRead));
        assert_eq!(FileEventType::from_u32(99), None);
    }

    #[test]
    fn test_fim_vs_access_events() {
        assert!(FileEventType::Create.is_fim_event());
        assert!(FileEventType::Modify.is_fim_event());
        assert!(!FileEventType::Access.is_fim_event());

        assert!(FileEventType::Access.is_access_event());
        assert!(FileEventType::OpenRead.is_access_event());
        assert!(FileEventType::OpenWrite.is_access_event());
        // OpenWrite is both FIM and access
        assert!(FileEventType::OpenWrite.is_fim_event());
    }
}
