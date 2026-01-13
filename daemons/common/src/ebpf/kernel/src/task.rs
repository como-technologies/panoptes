//! # Kernel Task Structure Access
//!
//! Provides access to kernel `task_struct` fields for process attribution.
//! Used by exec/exit tracepoints to capture process context at exec time.
//!
//! # Kernel Structure Offsets
//!
//! These offsets are for common kernel versions (5.x, 6.x). For true portability
//! across kernel versions, BTF/CO-RE should be used, but hardcoded offsets work
//! for the majority of production systems.
//!
//! # Safety
//!
//! All kernel memory access uses `bpf_probe_read_kernel` which is safe for
//! reading kernel memory from BPF context.

use aya_ebpf::helpers::{bpf_get_current_task, bpf_probe_read_kernel};
use panoptes_ebpf_types::{MAX_CMDLINE_LEN, MAX_CWD_LEN, MAX_EXE_LEN};

use crate::path::{extract_path_from_dentry, Dentry};

// ============================================================================
// Kernel Structure Offsets
// ============================================================================
// These offsets are for Linux 5.x/6.x kernels. They may need adjustment for
// other kernel versions. Consider using BTF/CO-RE for better portability.

/// Offset of `real_parent` in `task_struct`
/// task_struct->real_parent (pointer to parent task)
const TASK_REAL_PARENT_OFFSET: usize = 2336; // Approximate, varies by kernel config

/// Offset of `tgid` in `task_struct`
/// task_struct->tgid
const TASK_TGID_OFFSET: usize = 2284; // Approximate

/// Offset of `mm` in `task_struct`
/// task_struct->mm (pointer to mm_struct)
const TASK_MM_OFFSET: usize = 2104; // Approximate

/// Offset of `fs` in `task_struct`
/// task_struct->fs (pointer to fs_struct)
const TASK_FS_OFFSET: usize = 2656; // Approximate

/// Offset of `arg_start` in `mm_struct`
/// mm_struct->arg_start (start of command line args)
const MM_ARG_START_OFFSET: usize = 360; // Approximate

/// Offset of `arg_end` in `mm_struct`
/// mm_struct->arg_end (end of command line args)
const MM_ARG_END_OFFSET: usize = 368; // Approximate

/// Offset of `pwd` in `fs_struct`
/// fs_struct->pwd (current working directory path struct)
const FS_PWD_OFFSET: usize = 32; // Approximate

/// Maximum command line bytes to read
const MAX_CMDLINE_READ: usize = 128;

// ============================================================================
// Process Info Extraction
// ============================================================================

/// Extract parent process ID (ppid) from current task.
///
/// Reads task->real_parent->tgid to get the parent's thread group ID.
///
/// # Returns
///
/// Parent process tgid, or 0 on error.
#[inline(always)]
pub fn extract_ppid() -> u32 {
    unsafe {
        let task = bpf_get_current_task() as *const u8;
        if task.is_null() {
            return 0;
        }

        // Read real_parent pointer
        let parent_ptr = task.add(TASK_REAL_PARENT_OFFSET) as *const *const u8;
        let parent: *const u8 = match bpf_probe_read_kernel(parent_ptr) {
            Ok(p) => p,
            Err(_) => return 0,
        };

        if parent.is_null() {
            return 0;
        }

        // Read parent's tgid
        let tgid_ptr = parent.add(TASK_TGID_OFFSET) as *const u32;
        match bpf_probe_read_kernel(tgid_ptr) {
            Ok(tgid) => tgid,
            Err(_) => 0,
        }
    }
}

/// Extract current working directory from current task.
///
/// Reads task->fs->pwd and walks the dentry chain to get the full path.
///
/// # Arguments
///
/// * `out` - Buffer to write the cwd path (null-terminated)
///
/// # Returns
///
/// Length of the path written, or 0 on error.
#[inline(never)]
pub fn extract_cwd(out: &mut [u8; MAX_CWD_LEN]) -> usize {
    unsafe {
        let task = bpf_get_current_task() as *const u8;
        if task.is_null() {
            return 0;
        }

        // Read fs pointer (task->fs)
        let fs_ptr = task.add(TASK_FS_OFFSET) as *const *const u8;
        let fs: *const u8 = match bpf_probe_read_kernel(fs_ptr) {
            Ok(p) => p,
            Err(_) => return 0,
        };

        if fs.is_null() {
            return 0;
        }

        // Read pwd.dentry (fs->pwd is a path struct, dentry is at offset 8)
        let pwd_dentry_ptr = fs.add(FS_PWD_OFFSET + 8) as *const *const Dentry;
        let pwd_dentry: *const Dentry = match bpf_probe_read_kernel(pwd_dentry_ptr) {
            Ok(d) => d,
            Err(_) => return 0,
        };

        if pwd_dentry.is_null() {
            return 0;
        }

        // Use path extraction (reuse existing infrastructure)
        // Note: MAX_CWD_LEN is 128, MAX_PATH_LEN is 256, need to handle size difference
        let mut path_buf = [0u8; 256];
        let len = extract_path_from_dentry(pwd_dentry, &mut path_buf);

        // Copy to output, truncating if needed
        let copy_len = len.min(MAX_CWD_LEN - 1);
        let mut i = 0u32;
        loop {
            if i >= copy_len as u32 {
                break;
            }
            out[i as usize] = path_buf[i as usize];
            i += 1;
        }
        if copy_len < MAX_CWD_LEN {
            out[copy_len] = 0;
        }

        copy_len
    }
}

/// Extract command line from current task.
///
/// Reads task->mm->arg_start to arg_end and copies to buffer.
/// Arguments are null-separated in memory; we convert to space-separated.
///
/// # Arguments
///
/// * `out` - Buffer to write the cmdline (space-separated, null-terminated)
///
/// # Returns
///
/// Length of the cmdline written, or 0 on error.
///
/// # Note
///
/// This is complex and may not work in all contexts (e.g., kernel threads
/// have no mm). Falls back gracefully to empty string.
#[inline(never)]
pub fn extract_cmdline(out: &mut [u8; MAX_CMDLINE_LEN]) -> usize {
    unsafe {
        let task = bpf_get_current_task() as *const u8;
        if task.is_null() {
            return 0;
        }

        // Read mm pointer (task->mm)
        let mm_ptr = task.add(TASK_MM_OFFSET) as *const *const u8;
        let mm: *const u8 = match bpf_probe_read_kernel(mm_ptr) {
            Ok(p) => p,
            Err(_) => return 0,
        };

        // Kernel threads have no mm
        if mm.is_null() {
            return 0;
        }

        // Read arg_start and arg_end
        let arg_start_ptr = mm.add(MM_ARG_START_OFFSET) as *const u64;
        let arg_start: u64 = match bpf_probe_read_kernel(arg_start_ptr) {
            Ok(v) => v,
            Err(_) => return 0,
        };

        let arg_end_ptr = mm.add(MM_ARG_END_OFFSET) as *const u64;
        let arg_end: u64 = match bpf_probe_read_kernel(arg_end_ptr) {
            Ok(v) => v,
            Err(_) => return 0,
        };

        if arg_start == 0 || arg_end == 0 || arg_end <= arg_start {
            return 0;
        }

        // Calculate length to read (bounded)
        let total_len = (arg_end - arg_start) as usize;
        let read_len = total_len.min(MAX_CMDLINE_READ);

        // Read command line bytes
        // Note: This reads from user memory via kernel pointer, which may fail
        // in some contexts. We use bounded loop for verifier.
        let mut pos = 0usize;
        let mut i = 0u32;

        loop {
            if i >= read_len as u32 || pos >= MAX_CMDLINE_LEN - 1 {
                break;
            }

            let byte_ptr = (arg_start as *const u8).add(i as usize);
            let b: u8 = match bpf_probe_read_kernel(byte_ptr) {
                Ok(b) => b,
                Err(_) => break,
            };

            // Convert null separators to spaces (except trailing)
            if b == 0 {
                // Don't add trailing space
                if i + 1 < read_len as u32 {
                    out[pos] = b' ';
                    pos += 1;
                }
            } else {
                out[pos] = b;
                pos += 1;
            }

            i += 1;
        }

        // Null terminate
        if pos < MAX_CMDLINE_LEN {
            out[pos] = 0;
        }

        pos
    }
}

/// Extract executable path from a bprm filename pointer.
///
/// This is called from sched_process_exec tracepoint where we have access
/// to the linux_binprm->filename.
///
/// # Arguments
///
/// * `filename_ptr` - Pointer to the filename string (from bprm)
/// * `out` - Buffer to write the exe path (null-terminated)
///
/// # Returns
///
/// Length of the exe path written, or 0 on error.
#[inline(never)]
pub fn extract_exe_from_bprm(filename_ptr: *const u8, out: &mut [u8; MAX_EXE_LEN]) -> usize {
    if filename_ptr.is_null() {
        return 0;
    }

    let mut len = 0usize;
    let mut i = 0u32;

    // Bounded loop for BPF verifier
    loop {
        if i >= MAX_EXE_LEN as u32 - 1 {
            break;
        }

        let b: u8 = unsafe {
            match bpf_probe_read_kernel(filename_ptr.add(i as usize)) {
                Ok(b) => b,
                Err(_) => break,
            }
        };

        if b == 0 {
            break;
        }

        out[i as usize] = b;
        len = i as usize + 1;
        i += 1;
    }

    // Null terminate
    if len < MAX_EXE_LEN {
        out[len] = 0;
    }

    len
}
