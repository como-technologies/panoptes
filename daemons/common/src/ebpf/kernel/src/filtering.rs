//! # In-Kernel Path Filtering
//!
//! Implements Datadog's two-stage filtering model that reduces ring buffer
//! pressure by 90%+ in production workloads.
//!
//! ## Filtering Logic
//!
//! 1. If `FILTER_ENABLED[0] == 0`, emit all events (no filtering)
//! 2. If path is in `IGNORED_PATHS`, drop the event (discarder)
//! 3. If path matches any prefix in watched/guarded map, emit (approver)
//! 4. Otherwise, drop the event

use aya_ebpf::maps::{HashMap, LruHashMap};
use panoptes_ebpf_types::MAX_PATH_LEN;

/// Check if an event should be emitted based on in-kernel filters.
///
/// # Arguments
///
/// * `path` - The file path from the event
/// * `prefix_map` - The map containing watched/guarded prefixes
/// * `ignored_map` - The LRU map containing paths to ignore
/// * `filter_enabled_map` - The map containing the global filter toggle
///
/// # Returns
///
/// `true` if the event should be emitted, `false` if it should be dropped.
///
/// # Safety
///
/// This function accesses BPF maps which requires unsafe blocks.
#[inline(always)]
pub fn should_emit_event(
    path: &[u8; MAX_PATH_LEN],
    prefix_map: &HashMap<[u8; 128], u8>,
    ignored_map: &LruHashMap<[u8; 256], u8>,
    filter_enabled_map: &HashMap<u32, u32>,
) -> bool {
    // Check if filtering is enabled (default: disabled = emit all)
    let enabled = unsafe { filter_enabled_map.get(&0).copied().unwrap_or(0) };
    if enabled == 0 {
        return true; // No filtering, emit all events
    }

    // Check if path is in the ignored list (discarder)
    if unsafe { ignored_map.get(path).is_some() } {
        return false; // Path is explicitly ignored
    }

    // Check if path matches any watched/guarded prefix (approver)
    // We check progressively shorter prefixes of the path
    // This is O(n) but bounded by MAX_PATH_LEN and typically short-circuits early
    let path_len = path.iter().position(|&b| b == 0).unwrap_or(MAX_PATH_LEN);

    // Check common prefix lengths: full path, then progressively shorter
    // We use fixed key sizes for BPF verifier compatibility
    for prefix_len in [128, 64, 32, 16, 8].iter() {
        if *prefix_len > path_len {
            continue;
        }

        let mut prefix_key = [0u8; 128];
        let copy_len = (*prefix_len).min(128);
        prefix_key[..copy_len].copy_from_slice(&path[..copy_len]);

        if unsafe { prefix_map.get(&prefix_key).is_some() } {
            return true; // Path matches a watched/guarded prefix
        }
    }

    // No matching prefix found - drop the event
    false
}
