//! # Process Tracking Tracepoints
//!
//! Provides exec/exit tracepoint handlers for process cache management.
//! These handlers populate and clean up the PROCESS_CACHE map.
//!
//! # Usage
//!
//! Each daemon eBPF program should:
//! 1. Call `define_process_cache_maps!()` to create the maps
//! 2. Call `define_process_tracepoints!()` to create the handlers
//!
//! ```rust,ignore
//! use panoptes_ebpf_kernel::{define_filter_maps, define_process_cache_maps};
//!
//! // Define filter maps
//! define_filter_maps!(WATCHED_PREFIXES);
//!
//! // Define process cache maps and tracepoints
//! define_process_cache_maps!();
//! define_process_tracepoints!();
//! ```

/// Define tracepoint handlers for process tracking.
///
/// This macro creates:
/// - `sched_process_exec`: Populates PROCESS_CACHE with exe, cmdline, cwd, ppid
/// - `sched_process_exit`: Removes entry from PROCESS_CACHE
///
/// # Requirements
///
/// Must be used after `define_process_cache_maps!()` which creates:
/// - `PROCESS_CACHE`: LruHashMap for process context
/// - `PROCESS_CACHE_SCRATCH`: Per-CPU scratch space
#[macro_export]
macro_rules! define_process_tracepoints {
    () => {
        /// Tracepoint: sched_process_exec
        ///
        /// Called when a process calls execve(). Captures process context at exec time
        /// and stores in PROCESS_CACHE for later lookup by file event hooks.
        #[::aya_ebpf::macros::tracepoint(category = "sched", name = "sched_process_exec")]
        pub fn sched_process_exec(ctx: ::aya_ebpf::programs::TracePointContext) -> i32 {
            let _ = try_handle_exec(&ctx);
            0
        }

        #[inline(always)]
        fn try_handle_exec(ctx: &::aya_ebpf::programs::TracePointContext) -> Result<(), ()> {
            use ::aya_ebpf::helpers::{
                bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_get_current_uid_gid,
                bpf_ktime_get_ns,
            };
            use $crate::{
                MAX_COMM_LEN, ProcessCacheEntry, extract_cmdline, extract_cwd, extract_ppid,
            };

            // Get per-CPU scratch space
            let entry = PROCESS_CACHE_SCRATCH.get_ptr_mut(0).ok_or(())?;
            let entry = unsafe { &mut *entry };

            // Zero the entry
            *entry = ProcessCacheEntry::default();

            // Get tgid (cache key)
            let pid_tgid = bpf_get_current_pid_tgid();
            entry.tgid = (pid_tgid >> 32) as u32;

            // Get ppid from task->real_parent->tgid
            entry.ppid = extract_ppid();

            // Get uid/gid
            let uid_gid = bpf_get_current_uid_gid();
            entry.uid = uid_gid as u32;
            entry.gid = (uid_gid >> 32) as u32;

            // Get timestamp
            entry.exec_timestamp_ns = unsafe { bpf_ktime_get_ns() };

            // Get comm
            if let Ok(comm) = bpf_get_current_comm() {
                let len = comm.len().min(MAX_COMM_LEN);
                entry.comm[..len].copy_from_slice(&comm[..len]);
            }

            // Extract exe from tracepoint context
            // sched_process_exec provides filename at offset (varies by kernel)
            // For now, we'll try to read it from context
            // The filename pointer is typically at a fixed offset in the context
            let filename_ptr: *const u8 = unsafe {
                // Try to read filename pointer from tracepoint args
                // This offset may vary - common offset for filename in sched_process_exec
                ctx.read_at(16).unwrap_or(core::ptr::null())
            };
            if !filename_ptr.is_null() {
                $crate::extract_exe_from_bprm(filename_ptr, &mut entry.exe);
            }

            // Extract cmdline and cwd
            extract_cmdline(&mut entry.cmdline);
            extract_cwd(&mut entry.cwd);

            // Insert into cache
            let tgid = entry.tgid;
            unsafe {
                let _ = PROCESS_CACHE.insert(&tgid, entry, 0);
            }

            Ok(())
        }

        /// Tracepoint: sched_process_exit
        ///
        /// Called when a process exits. Removes the entry from PROCESS_CACHE
        /// to prevent stale entries and free memory.
        ///
        /// Only removes when the thread group leader exits (pid == tgid).
        #[::aya_ebpf::macros::tracepoint(category = "sched", name = "sched_process_exit")]
        pub fn sched_process_exit(_ctx: ::aya_ebpf::programs::TracePointContext) -> i32 {
            let _ = try_handle_exit();
            0
        }

        #[inline(always)]
        fn try_handle_exit() -> Result<(), ()> {
            use ::aya_ebpf::helpers::bpf_get_current_pid_tgid;

            let pid_tgid = bpf_get_current_pid_tgid();
            let tgid = (pid_tgid >> 32) as u32;
            let pid = pid_tgid as u32;

            // Only remove when thread group leader exits
            // This prevents premature removal when threads exit
            if pid == tgid {
                unsafe {
                    let _ = PROCESS_CACHE.remove(&tgid);
                }
            }

            Ok(())
        }
    };
}

/// Look up cached process info for the current process.
///
/// This function should be called from LSM hooks to enrich file events
/// with process context.
///
/// # Arguments
///
/// * `cache` - Reference to the PROCESS_CACHE map
///
/// # Returns
///
/// Option containing the cached ProcessCacheEntry, or None if not found.
///
/// # Note
///
/// This is a helper function, not a macro. Daemons should call this
/// inline in their LSM hooks.
#[macro_export]
macro_rules! lookup_process_cache {
    ($cache:expr) => {{
        let pid_tgid = ::aya_ebpf::helpers::bpf_get_current_pid_tgid();
        let tgid = (pid_tgid >> 32) as u32;
        unsafe { $cache.get(&tgid) }
    }};
}
