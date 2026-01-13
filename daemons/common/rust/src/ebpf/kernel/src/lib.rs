//! # Panoptes eBPF Common Library
//!
//! Shared eBPF kernel code for Argus (FIM) and Janus (access auditing) daemons.
//!
//! This crate provides common functionality used by both daemon's eBPF programs:
//!
//! - **Maps**: BPF map definitions via macros (ring buffer, filter maps)
//! - **Filtering**: In-kernel path filtering logic (approvers/discarders)
//! - **Helpers**: Process info population, event submission
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
//! use panoptes_ebpf_common::{
//!     define_filter_maps,
//!     populate_process_info,
//!     submit_event_filtered,
//! };
//!
//! // Define maps with daemon-specific prefix map name
//! define_filter_maps!(WATCHED_PREFIXES);
//!
//! // In your LSM hook:
//! let mut event = FileEvent::default();
//! populate_process_info(&mut event);
//! submit_event_filtered(&EVENTS, &event, &WATCHED_PREFIXES, &IGNORED_PATHS, &FILTER_ENABLED);
//! ```

#![no_std]

mod filtering;
mod helpers;
mod maps;
mod path;

// Re-export public APIs
pub use filtering::should_emit_event;
pub use helpers::{populate_process_info, submit_event, submit_event_filtered};
pub use path::{
    extract_path_from_dentry, extract_path_from_file, extract_path_with_name,
    Dentry, File, Path, QStr,
};
// Note: define_filter_maps is a #[macro_export] macro, automatically exported at crate root

// Re-export types for convenience
pub use panoptes_ebpf_types::{FileEvent, FileEventType, MAX_COMM_LEN, MAX_PATH_LEN};
