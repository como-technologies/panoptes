//! eBPF Support Detection (always compiled)
//!
//! This module provides kernel support detection for eBPF without requiring
//! the full eBPF feature flag. This enables runtime auto-detection.

/// Check if the current kernel supports eBPF LSM programs.
///
/// This checks for:
/// - Kernel version >= 5.7 (LSM BPF support)
/// - BTF availability at `/sys/kernel/btf/vmlinux`
/// - BPF LSM enabled (bpf in `/sys/kernel/security/lsm`)
///
/// # Returns
///
/// `true` if eBPF LSM programs can be loaded, `false` otherwise.
///
/// # Example
///
/// ```rust,ignore
/// use panoptes_common::ebpf_support::is_ebpf_supported;
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
    if !std::path::Path::new("/sys/kernel/btf/vmlinux").exists() {
        return false;
    }

    // Check if BPF LSM is enabled
    if !is_bpf_lsm_enabled() {
        return false;
    }

    true
}

/// Check if BPF LSM is enabled in the kernel.
///
/// Reads `/sys/kernel/security/lsm` to check if "bpf" is in the list of
/// enabled LSMs. BPF LSM requires:
/// - `CONFIG_BPF_LSM=y` in kernel config
/// - "bpf" in the LSM list (via boot param `lsm=...bpf`)
///
/// # Returns
///
/// `true` if BPF LSM is enabled, `false` otherwise.
pub fn is_bpf_lsm_enabled() -> bool {
    if let Ok(lsm) = std::fs::read_to_string("/sys/kernel/security/lsm") {
        lsm.split(',').any(|s| s.trim() == "bpf")
    } else {
        // Can't read LSM list - assume not supported
        false
    }
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

    #[test]
    fn test_bpf_lsm_check() {
        // This test depends on the actual kernel, so we just check it doesn't panic
        let _ = is_bpf_lsm_enabled();
    }
}
