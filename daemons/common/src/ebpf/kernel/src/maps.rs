//! # BPF Map Definitions
//!
//! Provides macros for defining common BPF maps used by both daemons.
//!
//! Each daemon uses a different prefix map name:
//! - Argus: `WATCHED_PREFIXES` (for watched paths)
//! - Janus: `GUARDED_PREFIXES` (for guarded paths)

/// Define the standard set of filter maps for a Panoptes daemon.
///
/// This macro creates:
/// - `EVENTS`: Ring buffer for sending events to userspace (256KB)
/// - `EVENT_SCRATCH`: Per-CPU array for FileEvent scratch space (avoids stack overflow)
/// - `$prefix_map_name`: HashMap for path prefixes to monitor
/// - `IGNORED_PATHS`: LruHashMap for paths to ignore (discarders)
/// - `FILTER_ENABLED`: HashMap for global filter toggle
///
/// # Usage
///
/// ```rust,ignore
/// // In argusd-ebpf:
/// define_filter_maps!(WATCHED_PREFIXES);
///
/// // In janusd-ebpf:
/// define_filter_maps!(GUARDED_PREFIXES);
/// ```
#[macro_export]
macro_rules! define_filter_maps {
    ($prefix_map_name:ident) => {
        /// Ring buffer for sending events to userspace
        /// Size: 256KB (can hold ~700 events before wraparound)
        #[::aya_ebpf::macros::map]
        static EVENTS: ::aya_ebpf::maps::RingBuf = ::aya_ebpf::maps::RingBuf::with_byte_size(256 * 1024, 0);

        /// Per-CPU scratch space for FileEvent
        /// Using PerCpuArray avoids stack overflow from the ~368 byte FileEvent struct
        #[::aya_ebpf::macros::map]
        static EVENT_SCRATCH: ::aya_ebpf::maps::PerCpuArray<$crate::FileEvent> =
            ::aya_ebpf::maps::PerCpuArray::with_max_entries(1, 0);

        /// Path prefixes to monitor - events only emitted if path matches one of these
        /// Key: path prefix (e.g., "/etc/", "/home/user/.ssh/")
        /// Value: 1 (presence indicates monitored)
        /// Populated by userspace when CreateWatch/CreateGuard RPC is called
        #[::aya_ebpf::macros::map]
        static $prefix_map_name: ::aya_ebpf::maps::HashMap<[u8; 128], u8> = ::aya_ebpf::maps::HashMap::with_max_entries(256, 0);

        /// Ignored paths - dynamically populated by userspace for paths that never match rules
        /// Uses LRU to automatically evict old entries when full
        /// Key: full path
        /// Value: 1 (presence indicates ignored)
        #[::aya_ebpf::macros::map]
        static IGNORED_PATHS: ::aya_ebpf::maps::LruHashMap<[u8; 256], u8> = ::aya_ebpf::maps::LruHashMap::with_max_entries(4096, 0);

        /// Global filter enable flag
        /// Key: 0 (single entry)
        /// Value: 0 = emit all events (no filtering), 1 = apply filters
        /// Allows fallback to unfiltered mode for debugging or initial sync
        #[::aya_ebpf::macros::map]
        static FILTER_ENABLED: ::aya_ebpf::maps::HashMap<u32, u32> = ::aya_ebpf::maps::HashMap::with_max_entries(1, 0);
    };
}

/// Define BPF maps for exec-time process caching.
///
/// This macro creates:
/// - `PROCESS_CACHE`: LruHashMap for process context (exe, cmdline, cwd, ppid)
/// - `PROCESS_CACHE_SCRATCH`: Per-CPU array for ProcessCacheEntry scratch space
///
/// # Architecture
///
/// ```text
/// Process exec ──► sched_process_exec ──► PROCESS_CACHE[tgid] = entry
///                                                │
/// File event   ──► LSM hook ◄─────────────lookup─┘
///                     │
/// Process exit ──► sched_process_exit ──► delete PROCESS_CACHE[tgid]
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// // In both argusd-ebpf and janusd-ebpf:
/// define_process_cache_maps!();
/// ```
///
/// # Memory
///
/// - 16,384 entries max * ~424 bytes = ~7MB kernel memory
/// - LRU eviction handles overflow gracefully
#[macro_export]
macro_rules! define_process_cache_maps {
    () => {
        /// Process cache - stores exec-time process context
        /// Key: tgid (thread group ID)
        /// Value: ProcessCacheEntry (exe, cmdline, cwd, ppid, etc.)
        /// LRU eviction when full (handles fork bombs, short-lived processes)
        #[::aya_ebpf::macros::map]
        static PROCESS_CACHE: ::aya_ebpf::maps::LruHashMap<u32, $crate::ProcessCacheEntry> =
            ::aya_ebpf::maps::LruHashMap::with_max_entries(16384, 0);

        /// Per-CPU scratch space for ProcessCacheEntry
        /// Using PerCpuArray avoids stack overflow from the ~424 byte struct
        /// (eBPF stack limit is 512 bytes, often 256 with tail calls)
        #[::aya_ebpf::macros::map]
        static PROCESS_CACHE_SCRATCH: ::aya_ebpf::maps::PerCpuArray<$crate::ProcessCacheEntry> =
            ::aya_ebpf::maps::PerCpuArray::with_max_entries(1, 0);
    };
}
