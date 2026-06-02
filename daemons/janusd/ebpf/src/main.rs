//! # Janus eBPF File Access Auditor
//!
//! Uses LSM hooks for file access auditing with permission control.
//! Uses per-CPU scratch buffer to avoid stack overflow.
//!
//! # Process Cache
//!
//! Includes exec/exit tracepoints to maintain a process cache for enriching
//! access events with cmdline, cwd, exe, and ppid. Userspace queries this cache
//! when processing events.

#![no_std]
#![no_main]

use aya_ebpf::macros::map;
use aya_ebpf::maps::HashMap;
use aya_ebpf::programs::LsmContext;

use panoptes_ebpf_kernel::{
    File, FileEvent, FileEventType, define_filter_maps, define_process_cache_maps,
    define_process_tracepoints, extract_path_from_file, populate_process_info,
    submit_event_filtered,
};

// Define maps including EVENT_SCRATCH per-CPU array
define_filter_maps!(GUARDED_PREFIXES);

// Define process cache maps and exec/exit tracepoints
define_process_cache_maps!();
define_process_tracepoints!();

/// Paths to deny access (kernel-level enforcement)
#[map]
static DENY_PATHS: HashMap<[u8; 256], u8> = HashMap::with_max_entries(1024, 0);

const EACCES: i32 = 13;
const MAY_WRITE: i32 = 2;

// ============================================================================
// LSM Hooks
// ============================================================================

/// security_file_open - file open with deny check
#[aya_ebpf::macros::lsm(hook = "file_open")]
pub fn file_open(ctx: LsmContext) -> i32 {
    match try_file_open(&ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

#[inline(always)]
fn try_file_open(ctx: &LsmContext) -> Result<i32, ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    *event = FileEvent::default();
    event.event_type = FileEventType::OpenRead as u32;
    populate_process_info(event);

    let file: *const File = unsafe { ctx.arg(0) };
    extract_path_from_file(file, &mut event.path);

    // Check deny list
    if event.path[0] != 0 {
        if unsafe { DENY_PATHS.get(&event.path).is_some() } {
            event.event_type = FileEventType::Access as u32;
            submit_event_filtered(
                &EVENTS,
                event,
                &GUARDED_PREFIXES,
                &IGNORED_PATHS,
                &FILTER_ENABLED,
            );
            return Ok(-EACCES);
        }
    }

    submit_event_filtered(
        &EVENTS,
        event,
        &GUARDED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(0)
}

/// security_file_permission - permission check with deny
#[aya_ebpf::macros::lsm(hook = "file_permission")]
pub fn file_permission(ctx: LsmContext) -> i32 {
    match try_file_permission(&ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

#[inline(always)]
fn try_file_permission(ctx: &LsmContext) -> Result<i32, ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    let mask: i32 = unsafe { ctx.arg(1) };

    *event = FileEvent::default();
    event.event_type = if mask & MAY_WRITE != 0 {
        FileEventType::OpenWrite as u32
    } else {
        FileEventType::Access as u32
    };
    populate_process_info(event);

    let file: *const File = unsafe { ctx.arg(0) };
    extract_path_from_file(file, &mut event.path);

    // Check deny list
    if event.path[0] != 0 {
        if unsafe { DENY_PATHS.get(&event.path).is_some() } {
            submit_event_filtered(
                &EVENTS,
                event,
                &GUARDED_PREFIXES,
                &IGNORED_PATHS,
                &FILTER_ENABLED,
            );
            return Ok(-EACCES);
        }
    }

    submit_event_filtered(
        &EVENTS,
        event,
        &GUARDED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
