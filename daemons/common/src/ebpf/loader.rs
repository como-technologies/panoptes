//! # eBPF Program Loader
//!
//! Generic loader for eBPF programs using the Aya framework.
//! Handles loading bytecode, attaching LSM hooks, and reading from ring buffers.

use aya::{
    Btf, Ebpf, EbpfLoader as AyaEbpfLoader,
    maps::{HashMap, RingBuf},
    programs::{Lsm, TracePoint},
};
use aya_log::EbpfLogger;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use panoptes_ebpf_types::{FileEvent, ProcessCacheEntry};

/// Errors from eBPF operations
#[derive(Error, Debug)]
pub enum EbpfError {
    /// Failed to load eBPF program
    #[error("Failed to load eBPF program: {0}")]
    Load(#[from] aya::EbpfError),

    /// Failed to attach LSM program
    #[error("Failed to attach LSM program: {0}")]
    Attach(String),

    /// Failed to access BPF map
    #[error("Failed to access map: {0}")]
    Map(String),

    /// eBPF not supported on this kernel
    #[error("eBPF not supported on this kernel (requires 5.7+ with BTF)")]
    NotSupported,

    /// Missing required capabilities
    #[error("Missing required capabilities (need CAP_BPF + CAP_PERFMON or CAP_SYS_ADMIN)")]
    MissingCapabilities,

    /// Failed to read eBPF bytecode
    #[error("Failed to read eBPF bytecode from {path}: {source}")]
    BytecodeRead {
        path: String,
        source: std::io::Error,
    },

    /// Event channel closed
    #[error("Event channel closed")]
    ChannelClosed,
}

/// Receiver for eBPF file events
pub struct EbpfEventReceiver {
    rx: mpsc::Receiver<FileEvent>,
}

impl EbpfEventReceiver {
    /// Create a new event receiver
    pub fn new(rx: mpsc::Receiver<FileEvent>) -> Self {
        Self { rx }
    }

    /// Receive the next event (blocking)
    pub async fn recv(&mut self) -> Option<FileEvent> {
        self.rx.recv().await
    }

    /// Try to receive an event without blocking
    pub fn try_recv(&mut self) -> Option<FileEvent> {
        self.rx.try_recv().ok()
    }
}

/// eBPF program loader and manager.
///
/// Handles loading eBPF bytecode, attaching to LSM hooks, and reading events
/// from the ring buffer.
///
/// # Usage
///
/// ```rust,ignore
/// // Load bytecode (daemon-specific path)
/// let bytecode = std::fs::read("path/to/daemon-ebpf")?;
///
/// // Load and attach programs
/// let (mut loader, receiver) = EbpfLoader::load_with_programs(
///     &bytecode,
///     &["inode_create", "inode_unlink", "file_open"],
/// )?;
///
/// // Start the event loop
/// loader.start_event_loop().await?;
///
/// // Receive events
/// while let Some(event) = receiver.recv().await {
///     println!("Event: {:?}", event);
/// }
/// ```
pub struct EbpfLoader {
    /// Loaded eBPF programs
    ebpf: Ebpf,
    /// Event sender for ring buffer events
    event_tx: mpsc::Sender<FileEvent>,
}

impl EbpfLoader {
    /// Load eBPF bytecode and attach specified LSM programs.
    ///
    /// # Arguments
    ///
    /// * `bytecode` - The eBPF program bytecode
    /// * `programs` - Names of LSM programs to attach (e.g., "inode_create", "file_open")
    ///
    /// # Returns
    ///
    /// Returns a tuple of (loader, event_receiver) on success.
    ///
    /// # Errors
    ///
    /// Returns `EbpfError` if:
    /// - eBPF is not supported on this kernel
    /// - Failed to load bytecode
    /// - Failed to attach programs
    pub fn load_with_programs(
        bytecode: &[u8],
        programs: &[&str],
    ) -> Result<(Self, EbpfEventReceiver), EbpfError> {
        // Check kernel support
        if !super::is_ebpf_supported() {
            return Err(EbpfError::NotSupported);
        }

        info!(programs = ?programs, "Loading eBPF programs");

        // Load BTF from sysfs
        let btf = Btf::from_sys_fs().map_err(|e| EbpfError::Load(aya::EbpfError::BtfError(e)))?;

        let mut ebpf = AyaEbpfLoader::new()
            .btf(Some(&btf))
            .load(bytecode)
            .map_err(EbpfError::Load)?;

        // Initialize eBPF logging
        if let Err(e) = EbpfLogger::init(&mut ebpf) {
            warn!(error = %e, "Failed to initialize eBPF logger (non-fatal)");
        }

        // Attach LSM programs
        for program_name in programs {
            Self::attach_lsm_program(&mut ebpf, &btf, program_name)?;
        }

        info!(
            count = programs.len(),
            "eBPF programs attached successfully"
        );

        // Create event channel (buffer 1024 events)
        let (event_tx, event_rx) = mpsc::channel(1024);

        let loader = Self { ebpf, event_tx };
        let receiver = EbpfEventReceiver::new(event_rx);

        Ok((loader, receiver))
    }

    /// Load eBPF bytecode from a file path.
    ///
    /// Convenience method that reads the bytecode from disk and calls
    /// `load_with_programs`.
    pub fn load_from_path(
        path: &str,
        programs: &[&str],
    ) -> Result<(Self, EbpfEventReceiver), EbpfError> {
        let bytecode = std::fs::read(path).map_err(|e| EbpfError::BytecodeRead {
            path: path.to_string(),
            source: e,
        })?;

        Self::load_with_programs(&bytecode, programs)
    }

    /// Attach a single LSM program by name.
    fn attach_lsm_program(ebpf: &mut Ebpf, btf: &Btf, name: &str) -> Result<(), EbpfError> {
        let program: &mut Lsm = ebpf
            .program_mut(name)
            .ok_or_else(|| EbpfError::Attach(format!("Program '{}' not found in bytecode", name)))?
            .try_into()
            .map_err(|e| EbpfError::Attach(format!("'{}' is not an LSM program: {}", name, e)))?;

        program
            .load(name, btf)
            .map_err(|e| EbpfError::Attach(format!("Failed to load '{}': {}", name, e)))?;

        program
            .attach()
            .map_err(|e| EbpfError::Attach(format!("Failed to attach '{}': {}", name, e)))?;

        debug!(program = name, "Attached LSM program");
        Ok(())
    }

    /// Start processing events from the ring buffer.
    ///
    /// Spawns a background task that reads events from the eBPF ring buffer
    /// and sends them to the event channel.
    ///
    /// # Ring Buffer
    ///
    /// The ring buffer map must be named "EVENTS" in the eBPF program.
    /// Events are FileEvent structs (see `panoptes-ebpf-types`).
    pub async fn start_event_loop(&mut self) -> Result<(), EbpfError> {
        // Get the ring buffer map
        let ring_buf: RingBuf<_> = self
            .ebpf
            .take_map("EVENTS")
            .ok_or_else(|| EbpfError::Map("EVENTS map not found in eBPF program".into()))?
            .try_into()
            .map_err(|e: aya::maps::MapError| {
                EbpfError::Map(format!("EVENTS is not a ring buffer: {}", e))
            })?;

        let tx = self.event_tx.clone();

        info!("Starting eBPF event loop");

        // Spawn blocking task to poll ring buffer
        // Note: In production, this would use async ring buffer polling with epoll/io_uring
        tokio::task::spawn_blocking(move || {
            let mut ring_buf = ring_buf;
            loop {
                // Poll ring buffer for events
                while let Some(item) = ring_buf.next() {
                    let data = item.as_ref();
                    if data.len() >= std::mem::size_of::<FileEvent>() {
                        // Safety: FileEvent is repr(C) and we checked the size
                        let event =
                            unsafe { std::ptr::read_unaligned(data.as_ptr() as *const FileEvent) };

                        // Use blocking send since we're in spawn_blocking
                        if tx.blocking_send(event).is_err() {
                            error!("Event channel closed, stopping eBPF event loop");
                            return;
                        }
                    }
                }

                // Small sleep to avoid busy loop (10ms)
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });

        Ok(())
    }

    /// Get a reference to the underlying Ebpf object.
    ///
    /// Useful for accessing maps or programs directly.
    pub fn ebpf(&self) -> &Ebpf {
        &self.ebpf
    }

    /// Get a mutable reference to the underlying Ebpf object.
    pub fn ebpf_mut(&mut self) -> &mut Ebpf {
        &mut self.ebpf
    }

    // ========================================================================
    // In-Kernel Filtering Map Management
    // ========================================================================
    //
    // These methods manage BPF maps for in-kernel event filtering:
    // - FILTER_ENABLED: Global toggle (0 = emit all, 1 = apply filters)
    // - WATCHED_PREFIXES/GUARDED_PREFIXES: Path prefixes to monitor
    // - IGNORED_PATHS: Paths to discard (never match rules)
    //
    // This implements Datadog's two-stage filtering model that reduces
    // ring buffer pressure by 90%+ in production workloads.
    // ========================================================================

    /// Get a mutable HashMap reference for a named map.
    fn get_hash_map_mut<const N: usize>(
        &mut self,
        map_name: &str,
    ) -> Result<HashMap<&mut aya::maps::MapData, [u8; N], u8>, EbpfError> {
        self.ebpf
            .map_mut(map_name)
            .ok_or_else(|| EbpfError::Map(format!("{} map not found", map_name)))?
            .try_into()
            .map_err(|e: aya::maps::MapError| {
                EbpfError::Map(format!("{} is not a HashMap: {}", map_name, e))
            })
    }

    /// Convert a path string to a fixed-size byte array key.
    fn path_to_key<const N: usize>(path: &str) -> [u8; N] {
        let mut key = [0u8; N];
        let bytes = path.as_bytes();
        let len = bytes.len().min(N);
        key[..len].copy_from_slice(&bytes[..len]);
        key
    }

    /// Enable or disable in-kernel path filtering.
    ///
    /// When disabled (default), all events are emitted to userspace.
    /// When enabled, only events matching watched prefixes (and not in
    /// ignored paths) are emitted.
    ///
    /// # Arguments
    ///
    /// * `enabled` - true to enable filtering, false to emit all events
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the FILTER_ENABLED map is not found or
    /// has the wrong type.
    pub fn set_filter_enabled(&mut self, enabled: bool) -> Result<(), EbpfError> {
        let mut map: HashMap<_, u32, u32> = self
            .ebpf
            .map_mut("FILTER_ENABLED")
            .ok_or_else(|| EbpfError::Map("FILTER_ENABLED map not found".into()))?
            .try_into()
            .map_err(|e: aya::maps::MapError| {
                EbpfError::Map(format!("FILTER_ENABLED is not a HashMap: {}", e))
            })?;

        map.insert(0, if enabled { 1 } else { 0 }, 0)
            .map_err(|e| EbpfError::Map(format!("Failed to update FILTER_ENABLED: {}", e)))?;

        info!(
            enabled = enabled,
            "In-kernel filtering {}",
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }

    /// Add a watched path prefix to the in-kernel filter.
    ///
    /// Events with paths matching this prefix will be emitted (when filtering
    /// is enabled). The prefix is truncated to 128 bytes if longer.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Path prefix to watch (e.g., "/etc/", "/home/user/.ssh/")
    /// * `map_name` - Name of the map ("WATCHED_PREFIXES" for Argus, "GUARDED_PREFIXES" for Janus)
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the map is not found or has the wrong type.
    pub fn add_watched_prefix(&mut self, prefix: &str, map_name: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<128>(map_name)?;
        let key = Self::path_to_key::<128>(prefix);

        map.insert(key, 1, 0)
            .map_err(|e| EbpfError::Map(format!("Failed to add prefix '{}': {}", prefix, e)))?;

        debug!(prefix = prefix, "Added watched prefix to kernel filter");
        Ok(())
    }

    /// Remove a watched path prefix from the in-kernel filter.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Path prefix to remove
    /// * `map_name` - Name of the map ("WATCHED_PREFIXES" for Argus, "GUARDED_PREFIXES" for Janus)
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the map is not found or has the wrong type.
    pub fn remove_watched_prefix(&mut self, prefix: &str, map_name: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<128>(map_name)?;
        let key = Self::path_to_key::<128>(prefix);

        map.remove(&key)
            .map_err(|e| EbpfError::Map(format!("Failed to remove prefix '{}': {}", prefix, e)))?;

        debug!(prefix = prefix, "Removed watched prefix from kernel filter");
        Ok(())
    }

    /// Add a path to the ignored list (discarder).
    ///
    /// Events for this exact path will be dropped in the kernel before
    /// reaching userspace. The kernel-side LruHashMap uses LRU eviction when full.
    ///
    /// # Arguments
    ///
    /// * `path` - Exact path to ignore
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the IGNORED_PATHS map is not found or
    /// has the wrong type.
    ///
    /// # Note
    ///
    /// The kernel map is an LruHashMap but we interact with it via HashMap API.
    /// LRU eviction is handled by the kernel when the map is full.
    pub fn add_ignored_path(&mut self, path: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<256>("IGNORED_PATHS")?;
        let key = Self::path_to_key::<256>(path);

        map.insert(key, 1, 0)
            .map_err(|e| EbpfError::Map(format!("Failed to add ignored path '{}': {}", path, e)))?;

        debug!(path = path, "Added path to kernel ignore list");
        Ok(())
    }

    /// Remove a path from the ignored list.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to remove from ignore list
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the map is not found or has the wrong type.
    pub fn remove_ignored_path(&mut self, path: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<256>("IGNORED_PATHS")?;
        let key = Self::path_to_key::<256>(path);

        map.remove(&key).map_err(|e| {
            EbpfError::Map(format!("Failed to remove ignored path '{}': {}", path, e))
        })?;

        debug!(path = path, "Removed path from kernel ignore list");
        Ok(())
    }

    /// Sync a list of watched prefixes to the kernel filter map.
    ///
    /// This is a convenience method that clears existing prefixes and
    /// adds all provided prefixes. Useful when syncing from CreateWatch/CreateGuard.
    ///
    /// # Arguments
    ///
    /// * `prefixes` - List of path prefixes to watch
    /// * `map_name` - Name of the map to sync
    ///
    /// # Note
    ///
    /// This does NOT clear existing entries - it only adds. For a full sync,
    /// use with set_filter_enabled(false), clear, add all, set_filter_enabled(true).
    pub fn sync_watched_prefixes(
        &mut self,
        prefixes: &[&str],
        map_name: &str,
    ) -> Result<(), EbpfError> {
        for prefix in prefixes {
            self.add_watched_prefix(prefix, map_name)?;
        }
        info!(
            count = prefixes.len(),
            map = map_name,
            "Synced watched prefixes to kernel filter"
        );
        Ok(())
    }

    // ========================================================================
    // Janus-Specific: DENY_PATHS Map Management
    // ========================================================================
    //
    // DENY_PATHS is Janus-specific - it contains paths that should be
    // blocked with -EACCES at the LSM level. This provides kernel-level
    // permission enforcement with no userspace race condition.
    // ========================================================================

    /// Add a path to the DENY_PATHS map (blocks access with -EACCES).
    ///
    /// This is Janus-specific functionality for permission control.
    /// When a path matches DENY_PATHS, the LSM hook returns -EACCES,
    /// blocking the file access at the kernel level.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to block
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the DENY_PATHS map is not found or
    /// has the wrong type.
    pub fn add_deny_path(&mut self, path: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<256>("DENY_PATHS")?;
        let key = Self::path_to_key::<256>(path);

        map.insert(key, 1, 0)
            .map_err(|e| EbpfError::Map(format!("Failed to add deny path '{}': {}", path, e)))?;

        debug!(path = path, "Added path to kernel DENY_PATHS");
        Ok(())
    }

    /// Remove a path from the DENY_PATHS map.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to remove from deny list
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the map is not found or has the wrong type.
    pub fn remove_deny_path(&mut self, path: &str) -> Result<(), EbpfError> {
        let mut map = self.get_hash_map_mut::<256>("DENY_PATHS")?;
        let key = Self::path_to_key::<256>(path);

        map.remove(&key)
            .map_err(|e| EbpfError::Map(format!("Failed to remove deny path '{}': {}", path, e)))?;

        debug!(path = path, "Removed path from kernel DENY_PATHS");
        Ok(())
    }

    // ========================================================================
    // Process Cache: Exec-Time Process Context
    // ========================================================================
    //
    // The PROCESS_CACHE map stores process context (exe, cmdline, cwd, ppid)
    // captured at exec time. The exec/exit tracepoints maintain this cache.
    // Userspace queries this map to enrich file events with process info.
    // ========================================================================

    /// Attach process tracking tracepoints (sched_process_exec, sched_process_exit).
    ///
    /// These tracepoints maintain the PROCESS_CACHE map with process context
    /// captured at exec time. Must be called after loading the eBPF bytecode
    /// that includes the `define_process_tracepoints!()` macro.
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Attach` if the tracepoints are not found in the
    /// bytecode or fail to attach.
    pub fn attach_process_tracepoints(&mut self) -> Result<(), EbpfError> {
        self.attach_tracepoint_if_exists("sched_process_exec", "sched", "process cache disabled")?;
        self.attach_tracepoint_if_exists(
            "sched_process_exit",
            "sched",
            "process cache cleanup disabled",
        )?;

        info!("Process tracking tracepoints attached");
        Ok(())
    }

    /// Attach a single tracepoint if it exists in the bytecode.
    ///
    /// # Arguments
    ///
    /// * `name` - Program name in bytecode
    /// * `category` - Tracepoint category (e.g., "sched")
    /// * `disabled_reason` - Log message if program not found
    fn attach_tracepoint_if_exists(
        &mut self,
        name: &str,
        category: &str,
        disabled_reason: &str,
    ) -> Result<(), EbpfError> {
        if let Some(prog) = self.ebpf.program_mut(name) {
            let prog: &mut TracePoint = prog
                .try_into()
                .map_err(|e| EbpfError::Attach(format!("{} is not a tracepoint: {}", name, e)))?;

            prog.load()
                .map_err(|e| EbpfError::Attach(format!("Failed to load {}: {}", name, e)))?;

            prog.attach(category, name)
                .map_err(|e| EbpfError::Attach(format!("Failed to attach {}: {}", name, e)))?;

            debug!(program = name, "Attached tracepoint");
        } else {
            debug!(
                program = name,
                reason = disabled_reason,
                "Tracepoint not found in bytecode"
            );
        }
        Ok(())
    }

    /// Get cached process info for a given thread group ID.
    ///
    /// Looks up the PROCESS_CACHE map for exec-time process context including
    /// exe, cmdline, cwd, and ppid.
    ///
    /// # Arguments
    ///
    /// * `tgid` - Thread group ID (same as pid for single-threaded processes)
    ///
    /// # Returns
    ///
    /// Returns `Some(ProcessCacheEntry)` if the process is in the cache,
    /// `None` if not found (e.g., process exited or was never cached).
    ///
    /// # Errors
    ///
    /// Returns `EbpfError::Map` if the PROCESS_CACHE map is not found.
    pub fn get_cached_process(&self, tgid: u32) -> Result<Option<ProcessCacheEntry>, EbpfError> {
        let map: HashMap<_, u32, ProcessCacheEntry> = self
            .ebpf
            .map("PROCESS_CACHE")
            .ok_or_else(|| EbpfError::Map("PROCESS_CACHE map not found".into()))?
            .try_into()
            .map_err(|e: aya::maps::MapError| {
                EbpfError::Map(format!("PROCESS_CACHE is not a HashMap: {}", e))
            })?;

        match map.get(&tgid, 0) {
            Ok(entry) => Ok(Some(entry)),
            Err(aya::maps::MapError::KeyNotFound) => Ok(None),
            Err(e) => Err(EbpfError::Map(format!(
                "Failed to lookup process {}: {}",
                tgid, e
            ))),
        }
    }

    /// Check if process cache is available.
    ///
    /// Returns true if the PROCESS_CACHE map exists in the loaded eBPF program.
    pub fn has_process_cache(&self) -> bool {
        self.ebpf.map("PROCESS_CACHE").is_some()
    }
}

impl Drop for EbpfLoader {
    fn drop(&mut self) {
        info!("Unloading eBPF programs");
        // Aya automatically detaches programs when Ebpf is dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebpf_error_display() {
        let err = EbpfError::NotSupported;
        assert!(err.to_string().contains("5.7"));

        let err = EbpfError::Attach("test".into());
        assert!(err.to_string().contains("test"));
    }
}
