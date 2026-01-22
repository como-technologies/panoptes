//! # eBPF Support for Panoptes Daemons
//!
//! This module provides shared eBPF infrastructure for both Argus (FIM) and
//! Janus (access auditing) daemons. It enables process attribution at the
//! kernel level, solving TOCTOU race conditions that occur with /proc lookups.
//!
//! ## Why eBPF?
//!
//! | Daemon | Kernel API | Native Process Info | Problem |
//! |--------|-----------|---------------------|---------|
//! | Argus  | inotify   | None                | No process attribution |
//! | Janus  | fanotify  | PID only            | TOCTOU race on /proc |
//!
//! eBPF LSM hooks capture process context (pid, uid, gid, comm) at the exact
//! moment of the file operation, eliminating race conditions.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │              eBPF Kernel Programs                       │
//! │  ┌─────────────────┐     ┌─────────────────┐           │
//! │  │ argusd-ebpf     │     │ janusd-ebpf     │           │
//! │  │ (LSM: create,   │     │ (LSM: file_open,│           │
//! │  │  unlink, rename)│     │  permission)    │           │
//! │  └────────┬────────┘     └────────┬────────┘           │
//! │           └───────────┬───────────┘                    │
//! │                ┌──────▼──────┐                         │
//! │                │ Ring Buffer │                         │
//! │                │ (256KB)     │                         │
//! │                └──────┬──────┘                         │
//! └───────────────────────│─────────────────────────────────┘
//!                         │
//! ┌───────────────────────│─────────────────────────────────┐
//! │              Userspace (this module)                    │
//! │                ┌──────▼──────┐                         │
//! │                │ EbpfLoader  │  (load, attach, poll)   │
//! │                └──────┬──────┘                         │
//! │                       │ FileEvent                      │
//! │           ┌───────────┴───────────┐                    │
//! │    ┌──────▼──────┐         ┌──────▼──────┐            │
//! │    │ argusd      │         │ janusd      │            │
//! │    │ events.rs   │         │ events.rs   │            │
//! │    │ (→proto)    │         │ (→proto)    │            │
//! │    └─────────────┘         └─────────────┘            │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Requirements
//!
//! - Linux kernel 5.7+ with `CONFIG_BPF_LSM=y`
//! - BTF enabled: `CONFIG_DEBUG_INFO_BTF=y`
//! - Capabilities: `CAP_BPF` + `CAP_PERFMON` (or `CAP_SYS_ADMIN`)
//!
//! ## Usage
//!
//! Each daemon uses this shared infrastructure but has its own:
//! - **Kernel programs** (`argusd-ebpf/`, `janusd-ebpf/`) - Different LSM hooks
//! - **Event conversion** (`src/ebpf/events.rs`) - Proto type mapping
//!
//! ```rust,ignore
//! use panoptes_common::ebpf::{is_ebpf_supported, EbpfLoader};
//!
//! if is_ebpf_supported() {
//!     let (loader, receiver) = EbpfLoader::load_and_attach(ebpf_bytecode)?;
//!     loader.start_event_loop().await?;
//!     while let Some(event) = receiver.recv().await {
//!         // Convert to daemon-specific proto
//!     }
//! }
//! ```
//!
//! ## Feature Flag
//!
//! This module requires the `ebpf` feature:
//! ```toml
//! panoptes-common = { path = "...", features = ["ebpf"] }
//! ```

mod loader;

pub use loader::{EbpfError, EbpfEventReceiver, EbpfLoader};

// Re-export shared types from panoptes-ebpf-types
pub use panoptes_ebpf_types::{
    FileEvent, FileEventType, MAX_COMM_LEN, MAX_CONTAINER_ID_LEN, MAX_PATH_LEN,
};

/// Check if the current kernel supports eBPF LSM programs.
///
/// This checks for:
/// - Kernel version >= 5.7 (LSM BPF support)
/// - BTF availability at `/sys/kernel/btf/vmlinux`
///
/// # Returns
///
/// `true` if eBPF LSM programs can be loaded, `false` otherwise.
///
/// # Example
///
/// ```rust,ignore
/// use panoptes_common::ebpf::is_ebpf_supported;
///
/// if !is_ebpf_supported() {
///     tracing::warn!("eBPF not supported, falling back to inotify/fanotify");
/// }
/// ```
pub fn is_ebpf_supported() -> bool {
    // Check kernel version >= 5.7
    if !check_kernel_version(5, 7) {
        return false;
    }

    // Check for BTF support
    std::path::Path::new("/sys/kernel/btf/vmlinux").exists()
}

/// Check if a specific kernel version is met.
fn check_kernel_version(required_major: u32, required_minor: u32) -> bool {
    if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let parts: Vec<&str> = release.trim().split('.').collect();
        if parts.len() >= 2 {
            if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return major > required_major
                    || (major == required_major && minor >= required_minor);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_version_check() {
        // This test depends on the actual kernel, so we just check it doesn't panic
        let _ = check_kernel_version(5, 7);
        let _ = is_ebpf_supported();
    }
}
