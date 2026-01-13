//! # Janus eBPF File Access Auditing
//!
//! This module provides Janus-specific eBPF integration for access audit events.
//! It uses the shared eBPF infrastructure from `panoptes-common` and provides
//! conversion to Janus protobuf types.
//!
//! ## Why eBPF for Janus?
//!
//! fanotify provides PID directly in events, but all other process info
//! (UID, GID, comm, exe, cmdline) must be looked up from /proc. If the
//! process exits before the lookup completes, we get incomplete data.
//!
//! eBPF solves this TOCTOU (time-of-check-to-time-of-use) race by capturing
//! all process context atomically at the exact moment of the file access.
//!
//! ## Feature Flag
//!
//! Enable with `cargo build --features ebpf`
//!
//! ## Usage
//!
//! ```rust,ignore
//! use panoptes_common::ebpf::{is_ebpf_supported, EbpfLoader};
//! use crate::ebpf::EbpfAccessEvent;
//!
//! if is_ebpf_supported() {
//!     let (loader, receiver) = EbpfLoader::load_from_path(
//!         "target/bpf/bpfel-unknown-none/release/janusd-ebpf",
//!         &["file_open", "file_permission"],
//!     )?;
//!     loader.start_event_loop().await?;
//!     while let Some(event) = receiver.recv().await {
//!         let janus_event = EbpfAccessEvent::from(event);
//!         // Convert to proto and send
//!     }
//! }
//! ```

mod events;

// Janus-specific event conversion
pub use events::EbpfAccessEvent;
