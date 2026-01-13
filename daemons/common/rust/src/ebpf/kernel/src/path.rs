//! # Path Extraction from Kernel Structures
//!
//! Provides path extraction from kernel dentry and file structures for LSM hooks.
//! Uses per-CPU maps for scratch space to stay within BPF's 512-byte stack limit.

use aya_ebpf::cty::c_uchar;
use aya_ebpf::helpers::bpf_probe_read_kernel;
use panoptes_ebpf_types::MAX_PATH_LEN;

/// Maximum depth of dentry chain to walk
const MAX_DEPTH: usize = 20;

/// Maximum component name length
const MAX_NAME: usize = 32;

// ============================================================================
// Kernel Structure Definitions
// ============================================================================

/// Kernel qstr
#[repr(C)]
pub struct QStr {
    pub hash_len: u64,
    pub name: *const c_uchar,
}

/// Kernel dentry
#[repr(C)]
pub struct Dentry {
    pub _pad: [u8; 24],
    pub d_parent: *const Dentry,
    pub d_name: QStr,
}

/// Kernel path
#[repr(C)]
pub struct Path {
    pub mnt: *const (),
    pub dentry: *const Dentry,
}

/// Kernel file
#[repr(C)]
pub struct File {
    pub _pad: [u8; 16],
    pub f_path: Path,
}

// ============================================================================
// Path Extraction
// ============================================================================

/// Extract path from dentry. Writes directly to caller's buffer.
#[inline(never)]
pub fn extract_path_from_dentry(dentry: *const Dentry, out: &mut [u8; MAX_PATH_LEN]) -> usize {
    if dentry.is_null() {
        return 0;
    }

    let mut pos = MAX_PATH_LEN;
    let mut cur = dentry;

    // Bounded walk - use explicit counter for verifier
    let mut depth = 0u32;
    loop {
        if depth >= MAX_DEPTH as u32 {
            break;
        }
        depth += 1;

        if cur.is_null() {
            break;
        }

        // Read dentry - minimal stack usage
        let parent: *const Dentry;
        let name_ptr: *const c_uchar;

        unsafe {
            let d_parent_ptr = (cur as *const u8).add(24) as *const *const Dentry;
            parent = match bpf_probe_read_kernel(d_parent_ptr) {
                Ok(p) => p,
                Err(_) => break,
            };

            // d_name.name is at offset 24 + 8 + 8 = 40
            let name_ptr_ptr = (cur as *const u8).add(40) as *const *const c_uchar;
            name_ptr = match bpf_probe_read_kernel(name_ptr_ptr) {
                Ok(p) => p,
                Err(_) => break,
            };
        }

        // Root check
        if parent == cur || parent.is_null() {
            break;
        }

        if name_ptr.is_null() {
            break;
        }

        // Get name length (inline, no function call)
        let mut nlen = 0usize;
        let mut ni = 0u32;
        loop {
            if ni >= MAX_NAME as u32 {
                break;
            }
            let b: u8 = unsafe {
                match bpf_probe_read_kernel(name_ptr.add(ni as usize)) {
                    Ok(b) => b,
                    Err(_) => break,
                }
            };
            if b == 0 {
                break;
            }
            nlen = ni as usize + 1;
            ni += 1;
        }

        if nlen == 0 {
            break;
        }

        // Need room for "/" + name
        if pos < nlen + 1 {
            break;
        }

        // Copy name to end of buffer
        pos -= nlen;
        let mut ci = 0u32;
        loop {
            if ci >= nlen as u32 || ci >= MAX_NAME as u32 {
                break;
            }
            let idx = pos + ci as usize;
            if idx >= MAX_PATH_LEN {
                break;
            }
            let b: u8 = unsafe {
                bpf_probe_read_kernel(name_ptr.add(ci as usize)).unwrap_or(0)
            };
            out[idx] = b;
            ci += 1;
        }

        // Add separator
        pos -= 1;
        out[pos] = b'/';

        cur = parent;
    }

    // Empty = root
    if pos == MAX_PATH_LEN {
        out[0] = b'/';
        out[1] = 0;
        return 1;
    }

    // Move to front
    let len = MAX_PATH_LEN - pos;
    let mut mi = 0u32;
    loop {
        if mi >= len as u32 || mi >= MAX_PATH_LEN as u32 {
            break;
        }
        let src = pos + mi as usize;
        if src >= MAX_PATH_LEN {
            break;
        }
        out[mi as usize] = out[src];
        mi += 1;
    }
    if len < MAX_PATH_LEN {
        out[len] = 0;
    }

    len
}

/// Extract path from file structure
#[inline(never)]
pub fn extract_path_from_file(file: *const File, out: &mut [u8; MAX_PATH_LEN]) -> usize {
    if file.is_null() {
        return 0;
    }

    // Read dentry pointer: file + 16 (pad) + 8 (mnt) = offset 24
    let dentry: *const Dentry = unsafe {
        let ptr = (file as *const u8).add(24) as *const *const Dentry;
        match bpf_probe_read_kernel(ptr) {
            Ok(d) => d,
            Err(_) => return 0,
        }
    };

    extract_path_from_dentry(dentry, out)
}

/// API compatibility
#[inline(always)]
pub fn extract_path_with_name(
    _dir: *const Dentry,
    dentry: *const Dentry,
    out: &mut [u8; MAX_PATH_LEN],
) -> usize {
    extract_path_from_dentry(dentry, out)
}
