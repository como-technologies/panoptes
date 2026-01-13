//! # Argus eBPF File Integrity Monitoring
//!
//! This module provides Argus-specific eBPF integration for file integrity events.
//! It uses the shared eBPF infrastructure from `panoptes-common` and provides
//! conversion to Argus protobuf types.
//!
//! ## Why eBPF for Argus?
//!
//! Traditional inotify provides WHAT changed but NOT who changed it - the kernel
//! doesn't include process info in inotify events. To get process attribution,
//! you'd need to guess based on timing, which is unreliable.
//!
//! eBPF LSM hooks capture process context (PID, UID, comm, container_id) atomically
//! at the exact moment of the file operation.
//!
//! ## Feature Flag
//!
//! Enable with `cargo build --features ebpf`
//!
//! ## Usage
//!
//! ```rust,ignore
//! use panoptes_common::ebpf::{is_ebpf_supported, EbpfLoader};
//! use crate::ebpf::EbpfFileEvent;
//!
//! if is_ebpf_supported() {
//!     let (loader, receiver) = EbpfLoader::load_from_path(
//!         "target/bpf/bpfel-unknown-none/release/argusd-ebpf",
//!         &["inode_create", "inode_unlink", "inode_rename"],
//!     )?;
//!     loader.start_event_loop().await?;
//!     while let Some(event) = receiver.recv().await {
//!         let argus_event = EbpfFileEvent::from(event);
//!         // Convert to proto and broadcast
//!     }
//! }
//! ```

mod events;

// Argus-specific event conversion
pub use events::EbpfFileEvent;
