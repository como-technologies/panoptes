//! # Kernel Structure Offsets
//!
//! Centralized definitions for kernel structure field offsets used by eBPF programs.
//!
//! # Why Hardcoded Offsets?
//!
//! The BPF verifier requires compile-time constant offsets for memory access.
//! While BTF/CO-RE provides runtime offset resolution, it requires:
//! - Kernel built with CONFIG_DEBUG_INFO_BTF=y
//! - BTF data available at /sys/kernel/btf/vmlinux
//!
//! Hardcoded offsets work on the majority of production systems running
//! common kernel configurations (5.x, 6.x x86_64).
//!
//! # Verifying Offsets
//!
//! Use `pahole` to verify offsets on your target kernel:
//!
//! ```bash
//! # Install pahole (dwarves package)
//! sudo apt install dwarves  # Debian/Ubuntu
//! sudo dnf install dwarves  # Fedora/RHEL
//!
//! # Dump task_struct layout
//! pahole -C task_struct /sys/kernel/btf/vmlinux 2>/dev/null | head -100
//!
//! # Find specific field offset
//! pahole -C task_struct /sys/kernel/btf/vmlinux 2>/dev/null | grep -E "real_parent|tgid|mm|fs"
//!
//! # Dump mm_struct layout
//! pahole -C mm_struct /sys/kernel/btf/vmlinux 2>/dev/null | grep -E "arg_start|arg_end"
//!
//! # Dump fs_struct layout
//! pahole -C fs_struct /sys/kernel/btf/vmlinux 2>/dev/null | grep pwd
//! ```
//!
//! # Kernel Version Compatibility
//!
//! These offsets are approximate for Linux 5.x/6.x kernels on x86_64.
//! Field positions vary based on:
//! - Kernel version
//! - Architecture (x86_64, aarch64)
//! - Kernel config options (CONFIG_*)
//! - Compiler and optimization flags
//!
//! # Safety
//!
//! All kernel memory access MUST use `bpf_probe_read_kernel` or the
//! `probe_kernel!` macro. Direct pointer dereference is undefined behavior
//! in BPF context.
//!
//! # References
//!
//! - Linux kernel source: include/linux/sched.h (task_struct)
//! - Linux kernel source: include/linux/mm_types.h (mm_struct)
//! - Linux kernel source: include/linux/fs_struct.h (fs_struct)
//! - BPF CO-RE: https://nakryiko.com/posts/bpf-core-reference-guide/

// ============================================================================
// task_struct Offsets
// ============================================================================

/// Offset of `real_parent` pointer in `task_struct`.
///
/// Points to the parent task that created this process.
/// Used to extract PPID (parent process ID).
///
/// ```c
/// // include/linux/sched.h
/// struct task_struct {
///     // ... many fields ...
///     struct task_struct *real_parent;  // at offset ~2336
///     // ...
/// };
/// ```
pub const TASK_REAL_PARENT_OFFSET: usize = 2336;

/// Offset of `tgid` (thread group ID) in `task_struct`.
///
/// The TGID is what userspace sees as the PID. All threads in a
/// process share the same TGID.
///
/// ```c
/// // include/linux/sched.h
/// struct task_struct {
///     // ...
///     pid_t tgid;  // at offset ~2284
///     // ...
/// };
/// ```
pub const TASK_TGID_OFFSET: usize = 2284;

/// Offset of `mm` pointer in `task_struct`.
///
/// Points to the memory descriptor (mm_struct) for userspace processes.
/// Kernel threads have mm == NULL.
///
/// ```c
/// // include/linux/sched.h
/// struct task_struct {
///     // ...
///     struct mm_struct *mm;  // at offset ~2104
///     // ...
/// };
/// ```
pub const TASK_MM_OFFSET: usize = 2104;

/// Offset of `fs` pointer in `task_struct`.
///
/// Points to filesystem information (current working directory, root).
///
/// ```c
/// // include/linux/sched.h
/// struct task_struct {
///     // ...
///     struct fs_struct *fs;  // at offset ~2656
///     // ...
/// };
/// ```
pub const TASK_FS_OFFSET: usize = 2656;

// ============================================================================
// mm_struct Offsets
// ============================================================================

/// Offset of `arg_start` in `mm_struct`.
///
/// Start address of the command line arguments in user memory.
///
/// ```c
/// // include/linux/mm_types.h
/// struct mm_struct {
///     // ...
///     unsigned long arg_start;  // at offset ~360
///     unsigned long arg_end;
///     // ...
/// };
/// ```
pub const MM_ARG_START_OFFSET: usize = 360;

/// Offset of `arg_end` in `mm_struct`.
///
/// End address of the command line arguments in user memory.
pub const MM_ARG_END_OFFSET: usize = 368;

// ============================================================================
// fs_struct Offsets
// ============================================================================

/// Offset of `pwd` (current working directory) in `fs_struct`.
///
/// The `pwd` field is a `struct path` containing vfsmount and dentry.
///
/// ```c
/// // include/linux/fs_struct.h
/// struct fs_struct {
///     int users;
///     spinlock_t lock;
///     seqcount_spinlock_t seq;
///     int umask;
///     int in_exec;
///     struct path root, pwd;  // pwd at offset ~32
/// };
///
/// // include/linux/path.h
/// struct path {
///     struct vfsmount *mnt;  // +0
///     struct dentry *dentry;  // +8
/// };
/// ```
///
/// To get the dentry, read at `FS_PWD_OFFSET + 8`.
pub const FS_PWD_OFFSET: usize = 32;

/// Offset of dentry within path struct.
/// Used as: `fs + FS_PWD_OFFSET + PATH_DENTRY_OFFSET` to get pwd dentry.
pub const PATH_DENTRY_OFFSET: usize = 8;
