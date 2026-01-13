//! # Panoptes eBPF Common Library
//!
//! Shared eBPF kernel code for Argus (FIM) and Janus (access auditing) daemons.
//!
//! This crate provides common functionality used by both daemon's eBPF programs:
//!
//! - **Maps**: BPF map definitions via macros (ring buffer, filter maps, process cache)
//! - **Filtering**: In-kernel path filtering logic (approvers/discarders)
//! - **Helpers**: Process info population, event submission
//! - **Process Cache**: Exec-time process context caching (cmdline, cwd, exe, ppid)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │              panoptes-ebpf-common (this crate)              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │ maps.rs     │  │ filtering.rs│  │ helpers.rs          │  │
//! │  │ (macros)    │  │ (logic)     │  │ (process info)      │  │
//! │  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
//! │         │                │                    │             │
//! │  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────────┴──────────┐  │
//! │  │ process.rs  │  │ task.rs     │  │ path.rs             │  │
//! │  │ (exec/exit) │  │ (task_struct│  │ (dentry walk)       │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! └─────────│────────────────│────────────────────│─────────────┘
//!           │                │                    │
//!     ┌─────▼────────────────▼────────────────────▼─────┐
//!     │              Daemon eBPF Programs                │
//!     │  ┌──────────────┐     ┌──────────────────┐      │
//!     │  │ argusd-ebpf  │     │ janusd-ebpf      │      │
//!     │  │ (FIM hooks)  │     │ (access hooks)   │      │
//!     │  └──────────────┘     └──────────────────┘      │
//!     └─────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use panoptes_ebpf_kernel::{
//!     define_filter_maps, define_process_cache_maps, define_process_tracepoints,
//!     populate_process_info, submit_event_filtered, lookup_process_cache,
//! };
//!
//! // Define maps with daemon-specific prefix map name
//! define_filter_maps!(WATCHED_PREFIXES);
//!
//! // Define process cache maps and exec/exit tracepoints
//! define_process_cache_maps!();
//! define_process_tracepoints!();
//!
//! // In your LSM hook:
//! let mut event = FileEvent::default();
//! populate_process_info(&mut event);
//!
//! // Look up cached process info (exe, cmdline, cwd, ppid)
//! if let Some(cached) = lookup_process_cache!(PROCESS_CACHE) {
//!     // Use cached.exe, cached.cmdline, cached.cwd, cached.ppid
//! }
//!
//! submit_event_filtered(&EVENTS, &event, &WATCHED_PREFIXES, &IGNORED_PATHS, &FILTER_ENABLED);
//! ```

#![no_std]

mod filtering;
mod helpers;
mod maps;
mod path;
mod process;
mod task;

// Re-export public APIs
pub use filtering::should_emit_event;
pub use helpers::{populate_process_info, submit_event, submit_event_filtered};
pub use path::{
    extract_path_from_dentry, extract_path_from_file, extract_path_with_name,
    Dentry, File, Path, QStr,
};
pub use task::{extract_cmdline, extract_cwd, extract_exe_from_bprm, extract_ppid};
// Note: define_filter_maps and define_process_cache_maps are #[macro_export] macros,
// automatically exported at crate root

// Re-export types for convenience
pub use panoptes_ebpf_types::{
    FileEvent, FileEventType, ProcessCacheEntry,
    MAX_CMDLINE_LEN, MAX_COMM_LEN, MAX_CWD_LEN, MAX_EXE_LEN, MAX_PATH_LEN,
};
