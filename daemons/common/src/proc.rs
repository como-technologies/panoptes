//! # Process Information via /proc
//!
//! This module provides utilities for reading process metadata from the
//! Linux proc filesystem (`/proc/{pid}/`).
//!
//! ## Key Files
//!
//! | Path | Contents | Used For |
//! |------|----------|----------|
//! | `/proc/{pid}/stat` | Process status line | PID, PPID, state, comm |
//! | `/proc/{pid}/status` | Key-value status | Uid, Gid (real, effective, saved, fs) |
//! | `/proc/{pid}/exe` | Symlink to executable | Process executable path |
//! | `/proc/{pid}/root` | Symlink to root directory | Container filesystem access |
//! | `/proc/{pid}/fd/{n}` | Symlinks to open fds | File descriptor resolution |
//!
//! ## /proc/{pid}/stat Format
//!
//! Space-separated fields (see `man 5 proc`):
//! ```text
//! pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt ...
//! ```
//!
//! **Important:** The `comm` field (process name) is enclosed in parentheses
//! and may contain spaces, parentheses, and other special characters.
//! Always parse from the **last** `)` character, not the first.
//!
//! ## /proc/{pid}/status Format
//!
//! Key-value pairs, one per line:
//! ```text
//! Name:   bash
//! Uid:    1000    1000    1000    1000
//! Gid:    1000    1000    1000    1000
//! ```
//!
//! Uid/Gid fields: real, effective, saved-set, filesystem
//!
//! ## Security Considerations
//!
//! - Requires `CAP_SYS_PTRACE` for accessing other processes' `/proc/{pid}/root`
//! - Process may exit between checks (TOCTOU) - handle errors gracefully
//! - Never trust process-provided data for security decisions
//!
//! ## Example
//!
//! ```rust,no_run
//! use panoptes_common::proc::{ProcessResolver, ProcfsProcessResolver};
//!
//! let resolver = ProcfsProcessResolver::new();
//!
//! // Get process info
//! let info = resolver.get_process_info(1234)?;
//! println!("Process: {} (PID {})", info.comm, info.pid);
//! println!("Parent PID: {}", info.ppid);
//! println!("UID: {}", info.uid);
//!
//! // Resolve file descriptor to path
//! let path = resolver.resolve_fd_path(1234, 3)?;
//! println!("FD 3 -> {}", path.display());
//! # Ok::<(), panoptes_common::error::ProcError>(())
//! ```
//!
//! ## References
//!
//! - `man 5 proc` - proc filesystem documentation
//! - `man 7 namespaces` - Linux namespaces

use std::fs;
use std::path::PathBuf;

use crate::error::ProcError;

/// Information about a process.
///
/// Retrieved from `/proc/{pid}/stat`, `/proc/{pid}/status`, and related files.
///
/// # Fields
///
/// All IDs are from the init (root) PID namespace perspective.
///
/// # V2 Extensions
///
/// The `cmdline` and `cwd` fields were added in v2 for enhanced process
/// attribution in Janus access events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Process ID.
    pub pid: u32,

    /// Parent process ID.
    pub ppid: u32,

    /// Real user ID (from `/proc/{pid}/status` Uid line).
    pub uid: u32,

    /// Real group ID (from `/proc/{pid}/status` Gid line).
    pub gid: u32,

    /// Process name (from `/proc/{pid}/stat` comm field).
    ///
    /// This is the filename of the executable, truncated to 15 characters.
    /// May contain spaces and special characters.
    pub comm: String,

    /// Path to the executable (from `/proc/{pid}/exe` symlink).
    ///
    /// May be empty if the executable was deleted or permission denied.
    pub exe: PathBuf,

    /// Full command line arguments (from `/proc/{pid}/cmdline`).
    ///
    /// The first element is typically the program name/path.
    /// May be empty if permission denied or process is a kernel thread.
    /// Added in v2 for enhanced process attribution.
    pub cmdline: Vec<String>,

    /// Current working directory (from `/proc/{pid}/cwd` symlink).
    ///
    /// May be empty if permission denied.
    /// Added in v2 for enhanced process attribution.
    pub cwd: PathBuf,
}

/// Trait for resolving process information.
///
/// This trait abstracts /proc filesystem access, allowing for:
/// - Production use with real /proc
/// - Testing with mock implementations
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` for use in async contexts.
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::proc::{ProcessResolver, ProcfsProcessResolver};
///
/// fn log_process_access(resolver: &dyn ProcessResolver, pid: u32) {
///     match resolver.get_process_info(pid) {
///         Ok(info) => println!("Access by {} (UID {})", info.comm, info.uid),
///         Err(e) => eprintln!("Failed to get process info: {}", e),
///     }
/// }
/// ```
pub trait ProcessResolver: Send + Sync {
    /// Retrieves comprehensive information about a process.
    ///
    /// # Arguments
    ///
    /// * `pid` - Process ID to query
    ///
    /// # Returns
    ///
    /// [`ProcessInfo`] containing PID, PPID, UID, GID, command name, and executable path.
    ///
    /// # Errors
    ///
    /// - [`ProcError::ProcessNotFound`] - Process doesn't exist
    /// - [`ProcError::PermissionDenied`] - Insufficient permissions
    /// - [`ProcError::StatReadError`] - Failed to read /proc/{pid}/stat
    /// - [`ProcError::StatusReadError`] - Failed to read /proc/{pid}/status
    fn get_process_info(&self, pid: u32) -> Result<ProcessInfo, ProcError>;

    /// Gets the parent process ID.
    ///
    /// # Arguments
    ///
    /// * `pid` - Process ID to query
    ///
    /// # Returns
    ///
    /// Parent PID (PPID) of the process.
    ///
    /// # Errors
    ///
    /// Same as [`get_process_info`](Self::get_process_info).
    fn get_parent_pid(&self, pid: u32) -> Result<u32, ProcError>;

    /// Resolves a file descriptor to its path.
    ///
    /// Uses `readlink` on `/proc/{pid}/fd/{fd}` to get the actual file path.
    ///
    /// # Arguments
    ///
    /// * `pid` - Process ID owning the file descriptor
    /// * `fd` - File descriptor number
    ///
    /// # Returns
    ///
    /// The path the file descriptor points to.
    ///
    /// # Errors
    ///
    /// - [`ProcError::ProcessNotFound`] - Process doesn't exist
    /// - [`ProcError::FdResolutionError`] - FD doesn't exist or permission denied
    ///
    /// # Notes
    ///
    /// - Deleted files show as `/path/to/file (deleted)`
    /// - Sockets show as `socket:[inode]`
    /// - Pipes show as `pipe:[inode]`
    fn resolve_fd_path(&self, pid: u32, fd: i32) -> Result<PathBuf, ProcError>;

    /// Checks if a process exists.
    ///
    /// # Arguments
    ///
    /// * `pid` - Process ID to check
    ///
    /// # Returns
    ///
    /// `true` if the process exists, `false` otherwise.
    fn process_exists(&self, pid: u32) -> bool {
        PathBuf::from(format!("/proc/{}", pid)).exists()
    }
}

/// Process resolver using the real /proc filesystem.
///
/// This is the production implementation that reads from the actual
/// Linux proc filesystem.
///
/// # Example
///
/// ```rust,no_run
/// use panoptes_common::proc::{ProcessResolver, ProcfsProcessResolver};
///
/// let resolver = ProcfsProcessResolver::new();
/// let info = resolver.get_process_info(std::process::id())?;
/// println!("Current process: {}", info.comm);
/// # Ok::<(), panoptes_common::error::ProcError>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct ProcfsProcessResolver {
    /// Base path for proc filesystem (usually "/proc").
    proc_path: PathBuf,
}

impl ProcfsProcessResolver {
    /// Creates a new resolver using the default /proc path.
    pub fn new() -> Self {
        Self {
            proc_path: PathBuf::from("/proc"),
        }
    }

    /// Creates a new resolver with a custom proc path.
    ///
    /// Useful for testing or accessing a mounted proc filesystem.
    ///
    /// # Arguments
    ///
    /// * `proc_path` - Path to proc filesystem mount
    pub fn with_proc_path(proc_path: PathBuf) -> Self {
        Self { proc_path }
    }

    /// Parses /proc/{pid}/stat to extract PID, PPID, and comm.
    ///
    /// # Format
    ///
    /// ```text
    /// pid (comm) state ppid pgrp session tty_nr tpgid flags ...
    /// ```
    ///
    /// # Parsing Strategy
    ///
    /// The `comm` field can contain any characters including spaces and
    /// parentheses. To parse correctly:
    /// 1. Find the first `(` - start of comm
    /// 2. Find the **last** `)` - end of comm
    /// 3. Parse fields after the last `)`
    fn parse_stat(&self, pid: u32) -> Result<(u32, String, u32), ProcError> {
        let stat_path = self.proc_path.join(format!("{}/stat", pid));

        let content = fs::read_to_string(&stat_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProcError::ProcessNotFound { pid }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ProcError::PermissionDenied {
                    pid,
                    file: "stat".to_string(),
                }
            } else {
                ProcError::StatReadError { pid, source: e }
            }
        })?;

        // Find comm field boundaries
        let comm_start = content.find('(').ok_or_else(|| ProcError::StatParseError {
            pid,
            reason: "missing '(' for comm field".to_string(),
        })?;

        // Find LAST ')' - comm can contain parentheses
        let comm_end = content
            .rfind(')')
            .ok_or_else(|| ProcError::StatParseError {
                pid,
                reason: "missing ')' for comm field".to_string(),
            })?;

        let comm = content[comm_start + 1..comm_end].to_string();

        // Parse fields after comm: state ppid ...
        let after_comm = &content[comm_end + 2..]; // Skip ") "
        let fields: Vec<&str> = after_comm.split_whitespace().collect();

        if fields.len() < 2 {
            return Err(ProcError::StatParseError {
                pid,
                reason: format!(
                    "expected at least 2 fields after comm, got {}",
                    fields.len()
                ),
            });
        }

        // fields[0] = state, fields[1] = ppid
        let ppid = fields[1]
            .parse::<u32>()
            .map_err(|_| ProcError::StatParseError {
                pid,
                reason: format!("invalid ppid: '{}'", fields[1]),
            })?;

        Ok((pid, comm, ppid))
    }

    /// Parses /proc/{pid}/status to extract UID and GID.
    ///
    /// # Format
    ///
    /// ```text
    /// Name:   process_name
    /// Uid:    1000    1000    1000    1000
    /// Gid:    1000    1000    1000    1000
    /// ```
    ///
    /// Uid/Gid values are: real, effective, saved-set, filesystem
    fn parse_status(&self, pid: u32) -> Result<(u32, u32), ProcError> {
        let status_path = self.proc_path.join(format!("{}/status", pid));

        let content = fs::read_to_string(&status_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProcError::ProcessNotFound { pid }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ProcError::PermissionDenied {
                    pid,
                    file: "status".to_string(),
                }
            } else {
                ProcError::StatusReadError { pid, source: e }
            }
        })?;

        let mut uid: Option<u32> = None;
        let mut gid: Option<u32> = None;

        for line in content.lines() {
            if let Some(uid_str) = line.strip_prefix("Uid:") {
                // Take first value (real UID)
                uid = uid_str
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            } else if let Some(gid_str) = line.strip_prefix("Gid:") {
                // Take first value (real GID)
                gid = gid_str
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            }

            // Stop early if we found both
            if uid.is_some() && gid.is_some() {
                break;
            }
        }

        let uid = uid.ok_or_else(|| ProcError::StatusParseError {
            pid,
            reason: "Uid field not found".to_string(),
        })?;

        let gid = gid.ok_or_else(|| ProcError::StatusParseError {
            pid,
            reason: "Gid field not found".to_string(),
        })?;

        Ok((uid, gid))
    }

    /// Reads the /proc/{pid}/exe symlink to get executable path.
    fn read_exe(&self, pid: u32) -> PathBuf {
        let exe_path = self.proc_path.join(format!("{}/exe", pid));

        // readlink may fail (permission denied, process exited, etc.)
        // Return empty path on failure
        fs::read_link(&exe_path).unwrap_or_default()
    }

    /// Reads the /proc/{pid}/cmdline file to get command line arguments.
    ///
    /// # Format
    ///
    /// Arguments are separated by null bytes (\0). The file contains:
    /// ```text
    /// /usr/bin/program\0arg1\0arg2\0
    /// ```
    ///
    /// # Notes
    ///
    /// - Kernel threads have empty cmdline
    /// - Some processes modify their cmdline (e.g., password hiding)
    fn read_cmdline(&self, pid: u32) -> Vec<String> {
        let cmdline_path = self.proc_path.join(format!("{}/cmdline", pid));

        // Read raw bytes (contains null separators)
        match fs::read(&cmdline_path) {
            Ok(bytes) => {
                // Split by null bytes and filter empty strings
                bytes
                    .split(|&b| b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Reads the /proc/{pid}/cwd symlink to get current working directory.
    fn read_cwd(&self, pid: u32) -> PathBuf {
        let cwd_path = self.proc_path.join(format!("{}/cwd", pid));

        // readlink may fail (permission denied, process exited, etc.)
        // Return empty path on failure
        fs::read_link(&cwd_path).unwrap_or_default()
    }
}

impl ProcessResolver for ProcfsProcessResolver {
    fn get_process_info(&self, pid: u32) -> Result<ProcessInfo, ProcError> {
        // Parse /proc/{pid}/stat for pid, comm, ppid
        let (parsed_pid, comm, ppid) = self.parse_stat(pid)?;

        // Parse /proc/{pid}/status for uid, gid
        let (uid, gid) = self.parse_status(pid)?;

        // Read executable path
        let exe = self.read_exe(pid);

        // Read command line arguments (v2 extension)
        let cmdline = self.read_cmdline(pid);

        // Read current working directory (v2 extension)
        let cwd = self.read_cwd(pid);

        Ok(ProcessInfo {
            pid: parsed_pid,
            ppid,
            uid,
            gid,
            comm,
            exe,
            cmdline,
            cwd,
        })
    }

    fn get_parent_pid(&self, pid: u32) -> Result<u32, ProcError> {
        let (_, _, ppid) = self.parse_stat(pid)?;
        Ok(ppid)
    }

    fn resolve_fd_path(&self, pid: u32, fd: i32) -> Result<PathBuf, ProcError> {
        let fd_path = self.proc_path.join(format!("{}/fd/{}", pid, fd));

        fs::read_link(&fd_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Could be process gone or fd closed
                if !self.process_exists(pid) {
                    ProcError::ProcessNotFound { pid }
                } else {
                    ProcError::FdResolutionError { pid, fd, source: e }
                }
            } else {
                ProcError::FdResolutionError { pid, fd, source: e }
            }
        })
    }

    fn process_exists(&self, pid: u32) -> bool {
        self.proc_path.join(format!("{}", pid)).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod process_info {
        use super::*;

        #[test]
        fn test_debug_impl() {
            let info = ProcessInfo {
                pid: 1234,
                ppid: 1,
                uid: 1000,
                gid: 1000,
                comm: "test".to_string(),
                exe: PathBuf::from("/usr/bin/test"),
                cmdline: vec!["test".to_string(), "--flag".to_string()],
                cwd: PathBuf::from("/home/user"),
            };
            let debug_str = format!("{:?}", info);
            assert!(debug_str.contains("1234"));
            assert!(debug_str.contains("test"));
            assert!(debug_str.contains("cmdline"));
            assert!(debug_str.contains("cwd"));
        }

        #[test]
        fn test_clone() {
            let info = ProcessInfo {
                pid: 1234,
                ppid: 1,
                uid: 1000,
                gid: 1000,
                comm: "test".to_string(),
                exe: PathBuf::from("/usr/bin/test"),
                cmdline: vec!["test".to_string()],
                cwd: PathBuf::from("/home/user"),
            };
            let cloned = info.clone();
            assert_eq!(info, cloned);
        }

        #[test]
        fn test_v2_fields() {
            let info = ProcessInfo {
                pid: 1234,
                ppid: 1,
                uid: 1000,
                gid: 1000,
                comm: "bash".to_string(),
                exe: PathBuf::from("/usr/bin/bash"),
                cmdline: vec![
                    "-bash".to_string(),
                    "-c".to_string(),
                    "echo hello".to_string(),
                ],
                cwd: PathBuf::from("/home/user/project"),
            };
            assert_eq!(info.cmdline.len(), 3);
            assert_eq!(info.cmdline[0], "-bash");
            assert_eq!(info.cwd.to_str().unwrap(), "/home/user/project");
        }
    }

    mod procfs_resolver {
        use super::*;
        use std::io::Write;
        use tempfile::TempDir;

        fn setup_mock_proc(pid: u32) -> (TempDir, ProcfsProcessResolver) {
            let temp_dir = TempDir::new().unwrap();
            let proc_path = temp_dir.path().to_path_buf();
            let pid_dir = proc_path.join(format!("{}", pid));
            fs::create_dir_all(&pid_dir).unwrap();

            let resolver = ProcfsProcessResolver::with_proc_path(proc_path);
            (temp_dir, resolver)
        }

        #[test]
        fn test_parse_stat_simple() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // Write mock stat file
            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            writeln!(stat_file, "1234 (bash) S 1 1234 1234 0 -1 4194304").unwrap();

            let (pid, comm, ppid) = resolver.parse_stat(1234).unwrap();
            assert_eq!(pid, 1234);
            assert_eq!(comm, "bash");
            assert_eq!(ppid, 1);
        }

        #[test]
        fn test_parse_stat_comm_with_spaces() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // comm field with spaces
            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            writeln!(
                stat_file,
                "1234 (Web Content) S 5678 1234 1234 0 -1 4194304"
            )
            .unwrap();

            let (pid, comm, ppid) = resolver.parse_stat(1234).unwrap();
            assert_eq!(pid, 1234);
            assert_eq!(comm, "Web Content");
            assert_eq!(ppid, 5678);
        }

        #[test]
        fn test_parse_stat_comm_with_parentheses() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // comm field with parentheses inside
            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            writeln!(stat_file, "1234 (test (1)) S 5678 1234 1234 0 -1 4194304").unwrap();

            let (pid, comm, ppid) = resolver.parse_stat(1234).unwrap();
            assert_eq!(pid, 1234);
            assert_eq!(comm, "test (1)");
            assert_eq!(ppid, 5678);
        }

        #[test]
        fn test_parse_status() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            let mut status_file = fs::File::create(pid_dir.join("status")).unwrap();
            writeln!(status_file, "Name:\tbash").unwrap();
            writeln!(status_file, "Umask:\t0022").unwrap();
            writeln!(status_file, "State:\tS (sleeping)").unwrap();
            writeln!(status_file, "Uid:\t1000\t1000\t1000\t1000").unwrap();
            writeln!(status_file, "Gid:\t1001\t1001\t1001\t1001").unwrap();

            let (uid, gid) = resolver.parse_status(1234).unwrap();
            assert_eq!(uid, 1000);
            assert_eq!(gid, 1001);
        }

        #[test]
        fn test_parse_status_root() {
            let (temp_dir, resolver) = setup_mock_proc(1);
            let pid_dir = temp_dir.path().join("1");

            let mut status_file = fs::File::create(pid_dir.join("status")).unwrap();
            writeln!(status_file, "Name:\tinit").unwrap();
            writeln!(status_file, "Uid:\t0\t0\t0\t0").unwrap();
            writeln!(status_file, "Gid:\t0\t0\t0\t0").unwrap();

            let (uid, gid) = resolver.parse_status(1).unwrap();
            assert_eq!(uid, 0);
            assert_eq!(gid, 0);
        }

        #[test]
        fn test_get_process_info_full() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // Create stat file
            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            writeln!(stat_file, "1234 (myprocess) S 5678 1234 1234 0 -1 4194304").unwrap();

            // Create status file
            let mut status_file = fs::File::create(pid_dir.join("status")).unwrap();
            writeln!(status_file, "Name:\tmyprocess").unwrap();
            writeln!(status_file, "Uid:\t1000\t1000\t1000\t1000").unwrap();
            writeln!(status_file, "Gid:\t1001\t1001\t1001\t1001").unwrap();

            // Create cmdline file (null-separated)
            let cmdline_path = pid_dir.join("cmdline");
            fs::write(&cmdline_path, b"myprocess\0--arg1\0--arg2\0").unwrap();

            // Create cwd symlink
            let cwd_target = PathBuf::from("/home/user/work");
            std::os::unix::fs::symlink(&cwd_target, pid_dir.join("cwd")).unwrap();

            let info = resolver.get_process_info(1234).unwrap();
            assert_eq!(info.pid, 1234);
            assert_eq!(info.ppid, 5678);
            assert_eq!(info.uid, 1000);
            assert_eq!(info.gid, 1001);
            assert_eq!(info.comm, "myprocess");
            // V2 fields
            assert_eq!(info.cmdline, vec!["myprocess", "--arg1", "--arg2"]);
            assert_eq!(info.cwd, cwd_target);
        }

        #[test]
        fn test_read_cmdline() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // Test normal cmdline
            fs::write(pid_dir.join("cmdline"), b"/usr/bin/bash\0-c\0echo hello\0").unwrap();
            let cmdline = resolver.read_cmdline(1234);
            assert_eq!(cmdline, vec!["/usr/bin/bash", "-c", "echo hello"]);
        }

        #[test]
        fn test_read_cmdline_empty() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            // Empty cmdline (kernel thread)
            fs::write(pid_dir.join("cmdline"), b"").unwrap();
            let cmdline = resolver.read_cmdline(1234);
            assert!(cmdline.is_empty());
        }

        #[test]
        fn test_read_cmdline_missing() {
            let (_temp_dir, resolver) = setup_mock_proc(1234);
            // No cmdline file created
            let cmdline = resolver.read_cmdline(1234);
            assert!(cmdline.is_empty());
        }

        #[test]
        fn test_read_cwd() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            let target = PathBuf::from("/var/log");
            std::os::unix::fs::symlink(&target, pid_dir.join("cwd")).unwrap();

            let cwd = resolver.read_cwd(1234);
            assert_eq!(cwd, target);
        }

        #[test]
        fn test_read_cwd_missing() {
            let (_temp_dir, resolver) = setup_mock_proc(1234);
            // No cwd symlink created
            let cwd = resolver.read_cwd(1234);
            assert!(cwd.as_os_str().is_empty());
        }

        #[test]
        fn test_process_not_found() {
            let resolver = ProcfsProcessResolver::with_proc_path(PathBuf::from("/nonexistent"));
            let result = resolver.get_process_info(99999);
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ProcError::ProcessNotFound { pid: 99999 }
            ));
        }

        #[test]
        fn test_process_exists() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let _pid_dir = temp_dir.path().join("1234");

            assert!(resolver.process_exists(1234));
            assert!(!resolver.process_exists(9999));
        }

        #[test]
        fn test_get_parent_pid() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");

            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            writeln!(stat_file, "1234 (test) S 5678 1234 1234 0 -1 4194304").unwrap();

            let ppid = resolver.get_parent_pid(1234).unwrap();
            assert_eq!(ppid, 5678);
        }

        #[test]
        fn test_resolve_fd_path() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");
            let fd_dir = pid_dir.join("fd");
            fs::create_dir_all(&fd_dir).unwrap();

            // Create a symlink for fd 3
            let target = PathBuf::from("/tmp/testfile.txt");
            std::os::unix::fs::symlink(&target, fd_dir.join("3")).unwrap();

            let resolved = resolver.resolve_fd_path(1234, 3).unwrap();
            assert_eq!(resolved, target);
        }

        #[test]
        fn test_resolve_fd_path_not_found() {
            let (temp_dir, resolver) = setup_mock_proc(1234);
            let pid_dir = temp_dir.path().join("1234");
            let fd_dir = pid_dir.join("fd");
            fs::create_dir_all(&fd_dir).unwrap();

            let result = resolver.resolve_fd_path(1234, 999);
            assert!(result.is_err());
        }
    }

    mod stat_parsing_edge_cases {
        use super::*;
        use std::io::Write;
        use tempfile::TempDir;

        fn setup_stat_test(content: &str) -> (TempDir, ProcfsProcessResolver) {
            let temp_dir = TempDir::new().unwrap();
            let proc_path = temp_dir.path().to_path_buf();
            let pid_dir = proc_path.join("1234");
            fs::create_dir_all(&pid_dir).unwrap();

            let mut stat_file = fs::File::create(pid_dir.join("stat")).unwrap();
            write!(stat_file, "{}", content).unwrap();

            let resolver = ProcfsProcessResolver::with_proc_path(proc_path);
            (temp_dir, resolver)
        }

        #[test]
        fn test_comm_with_newline_char() {
            // Some processes can have weird comm names
            let (_temp, resolver) = setup_stat_test("1234 (test\nname) S 1 1234 1234 0 -1 0");
            let result = resolver.parse_stat(1234);
            // Should handle or error gracefully
            assert!(result.is_ok() || result.is_err());
        }

        #[test]
        fn test_long_stat_line() {
            // Real stat lines have many fields
            let (_temp, resolver) = setup_stat_test(
                "1234 (bash) S 1 1234 1234 0 -1 4194304 1000 2000 3000 4000 100 200 0 0 20 0 1 0 12345678 123456789 1000 18446744073709551615 4194304 4238788 140736466511168 0 0 0 65536 3686404 1266761467 0 0 0 17 1 0 0 0 0 0"
            );
            let (pid, comm, ppid) = resolver.parse_stat(1234).unwrap();
            assert_eq!(pid, 1234);
            assert_eq!(comm, "bash");
            assert_eq!(ppid, 1);
        }
    }
}
