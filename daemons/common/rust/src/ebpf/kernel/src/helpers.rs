//! # eBPF Helper Functions
//!
//! Common helper functions for populating process info and submitting events.

use aya_ebpf::{
    helpers::{bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_get_current_uid_gid, bpf_ktime_get_ns},
    maps::{HashMap, LruHashMap, RingBuf},
};
use panoptes_ebpf_types::{FileEvent, MAX_COMM_LEN};

use crate::filtering::should_emit_event;

/// Populate process information in a FileEvent.
///
/// Extracts the following from the current task context:
/// - PID/TGID (process/thread group ID)
/// - UID/GID (user/group ID)
/// - Timestamp (kernel monotonic time)
/// - Command name (first 16 chars of process name)
///
/// # Arguments
///
/// * `event` - Mutable reference to the FileEvent to populate
#[inline(always)]
pub fn populate_process_info(event: &mut FileEvent) {
    // Get pid/tgid
    let pid_tgid = bpf_get_current_pid_tgid();
    event.tgid = (pid_tgid >> 32) as u32;
    event.pid = pid_tgid as u32;

    // Get uid/gid
    let uid_gid = bpf_get_current_uid_gid();
    event.uid = uid_gid as u32;
    event.gid = (uid_gid >> 32) as u32;

    // Get timestamp
    event.timestamp_ns = unsafe { bpf_ktime_get_ns() };

    // Get command name
    if let Ok(comm) = bpf_get_current_comm() {
        let len = comm.len().min(MAX_COMM_LEN);
        event.comm[..len].copy_from_slice(&comm[..len]);
    }
}

/// Submit an event to the ring buffer without filtering.
///
/// Use this when you've already checked filtering or don't need it.
///
/// # Arguments
///
/// * `ring_buf` - The ring buffer map to submit to
/// * `event` - The event to submit
#[inline(always)]
pub fn submit_event(ring_buf: &RingBuf, event: &FileEvent) {
    if let Some(mut buf) = ring_buf.reserve::<FileEvent>(0) {
        unsafe {
            buf.as_mut_ptr().write(*event);
        }
        buf.submit(0);
    }
}

/// Submit an event to the ring buffer with in-kernel filtering.
///
/// Checks should_emit_event() before submitting to reduce userspace load.
/// If path is not populated (all zeros), skips filtering to avoid false drops.
///
/// # Arguments
///
/// * `ring_buf` - The ring buffer map to submit to
/// * `event` - The event to submit
/// * `prefix_map` - The map containing watched/guarded prefixes
/// * `ignored_map` - The LRU map containing paths to ignore
/// * `filter_enabled_map` - The map containing the global filter toggle
#[inline(always)]
pub fn submit_event_filtered(
    ring_buf: &RingBuf,
    event: &FileEvent,
    prefix_map: &HashMap<[u8; 128], u8>,
    ignored_map: &LruHashMap<[u8; 256], u8>,
    filter_enabled_map: &HashMap<u32, u32>,
) {
    // Apply in-kernel filtering if path is populated
    // If path is empty (not yet extracted), we still emit to avoid losing events
    if event.path[0] != 0 && !should_emit_event(&event.path, prefix_map, ignored_map, filter_enabled_map) {
        return; // Event filtered out in kernel
    }

    submit_event(ring_buf, event);
}
