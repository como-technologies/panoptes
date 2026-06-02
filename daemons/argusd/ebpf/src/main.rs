//! # Argus eBPF File Integrity Monitor
//!
//! Uses LSM hooks to capture file operations with process context.
//! Uses per-CPU scratch buffer to avoid stack overflow.
//!
//! # Process Cache
//!
//! Includes exec/exit tracepoints to maintain a process cache for enriching
//! file events with cmdline, cwd, exe, and ppid. Userspace queries this cache
//! when processing events.

#![no_std]
#![no_main]

use aya_ebpf::programs::LsmContext;

use panoptes_ebpf_kernel::{
    Dentry, File, FileEvent, FileEventType, define_filter_maps, define_process_cache_maps,
    define_process_tracepoints, extract_path_from_dentry, extract_path_from_file,
    populate_process_info, submit_event_filtered,
};

// Define maps including EVENT_SCRATCH per-CPU array
define_filter_maps!(WATCHED_PREFIXES);

// Define process cache maps and exec/exit tracepoints
define_process_cache_maps!();
define_process_tracepoints!();

// ============================================================================
// LSM Hooks
// ============================================================================

/// security_inode_create - file creation
#[aya_ebpf::macros::lsm(hook = "inode_create")]
pub fn inode_create(ctx: LsmContext) -> i32 {
    let _ = try_inode_create(&ctx);
    0
}

#[inline(always)]
fn try_inode_create(ctx: &LsmContext) -> Result<(), ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    // Zero the event
    *event = FileEvent::default();
    event.event_type = FileEventType::Create as u32;
    populate_process_info(event);

    // Get dentry from arg2
    let dentry: *const Dentry = unsafe { ctx.arg(2) };
    extract_path_from_dentry(dentry, &mut event.path);

    submit_event_filtered(
        &EVENTS,
        event,
        &WATCHED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(())
}

/// security_inode_unlink - file deletion
#[aya_ebpf::macros::lsm(hook = "inode_unlink")]
pub fn inode_unlink(ctx: LsmContext) -> i32 {
    let _ = try_inode_unlink(&ctx);
    0
}

#[inline(always)]
fn try_inode_unlink(ctx: &LsmContext) -> Result<(), ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    *event = FileEvent::default();
    event.event_type = FileEventType::Delete as u32;
    populate_process_info(event);

    let dentry: *const Dentry = unsafe { ctx.arg(1) };
    extract_path_from_dentry(dentry, &mut event.path);

    submit_event_filtered(
        &EVENTS,
        event,
        &WATCHED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(())
}

/// security_inode_rename - file rename/move
#[aya_ebpf::macros::lsm(hook = "inode_rename")]
pub fn inode_rename(ctx: LsmContext) -> i32 {
    let _ = try_inode_rename(&ctx);
    0
}

#[inline(always)]
fn try_inode_rename(ctx: &LsmContext) -> Result<(), ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    // Emit event for old path (source)
    *event = FileEvent::default();
    event.event_type = FileEventType::Rename as u32;
    populate_process_info(event);

    let old_dentry: *const Dentry = unsafe { ctx.arg(1) };
    extract_path_from_dentry(old_dentry, &mut event.path);

    submit_event_filtered(
        &EVENTS,
        event,
        &WATCHED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );

    // Emit event for new path (destination)
    let new_dentry: *const Dentry = unsafe { ctx.arg(3) };
    event.event_type = FileEventType::Create as u32;
    extract_path_from_dentry(new_dentry, &mut event.path);

    submit_event_filtered(
        &EVENTS,
        event,
        &WATCHED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(())
}

/// security_file_open - file opened
#[aya_ebpf::macros::lsm(hook = "file_open")]
pub fn file_open(ctx: LsmContext) -> i32 {
    let _ = try_file_open(&ctx);
    0
}

#[inline(always)]
fn try_file_open(ctx: &LsmContext) -> Result<(), ()> {
    let event = EVENT_SCRATCH.get_ptr_mut(0).ok_or(())?;
    let event = unsafe { &mut *event };

    *event = FileEvent::default();
    event.event_type = FileEventType::OpenWrite as u32;
    populate_process_info(event);

    let file: *const File = unsafe { ctx.arg(0) };
    extract_path_from_file(file, &mut event.path);

    submit_event_filtered(
        &EVENTS,
        event,
        &WATCHED_PREFIXES,
        &IGNORED_PATHS,
        &FILTER_ENABLED,
    );
    Ok(())
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
