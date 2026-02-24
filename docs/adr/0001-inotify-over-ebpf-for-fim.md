# ADR-0001: inotify Over eBPF for File Integrity Monitoring

## Status

Accepted

## Context

Panoptes Argus needs a kernel mechanism for detecting file system changes in real-time. The two primary options are:

1. **inotify** — Linux kernel subsystem for monitoring filesystem events (available since Linux 2.6.13, 2005)
2. **eBPF** — Extended Berkeley Packet Filter with kprobes/tracepoints on VFS operations (requires Linux 5.x+ with BTF)

## Decision

We chose **inotify** as the primary FIM mechanism, with eBPF as an optional feature-flagged alternative for high-scale deployments.

## Rationale

**Why inotify:**

- **Universal availability**: Works on every Linux kernel since 2.6.13. No BTF, no CO-RE, no kernel headers required. This matters because many enterprise Kubernetes nodes run older or stripped-down kernels (Bottlerocket, Flatcar, Talos).
- **Simple mental model**: "Watch this path, get events when it changes." No BPF program compilation, no verifier constraints, no ring buffer management.
- **Recursive watching**: Native support for recursive directory monitoring with `IN_CREATE` + automatic child watch addition. eBPF VFS hooks require manual directory tree walking.
- **Battle-tested**: inotify has been in production for 20 years. The failure modes are well-understood (inotify limit exhaustion, race conditions on rapid renames). eBPF FIM is comparatively new territory.
- **Debuggability**: `cat /proc/sys/fs/inotify/max_user_watches` tells you your limit. inotify errors are straightforward. eBPF debugging requires bpftool, kernel logs, and verifier output interpretation.

**Why not eBPF-only:**

- Kernel version requirements exclude many production environments
- BPF program compilation adds build complexity (CO-RE or per-kernel builds)
- Process attribution (the main eBPF advantage) can be added via audit subsystem correlation
- The complexity/benefit ratio doesn't justify it as the default for FIM specifically

**Why eBPF as optional:**

- Higher event throughput at scale (ring buffer vs. inotify fd reads)
- Native process attribution without audit correlation
- Better suited for environments with 100k+ watched paths
- Feature-flagged in `Cargo.toml` as `ebpf` feature

## Consequences

- Argus works out-of-the-box on any Linux kernel 2.6.13+
- Operators must tune `fs.inotify.max_user_watches` for large deployments (documented in `docs/operations/kernel-tuning.md`)
- Process attribution requires separate implementation (audit correlation or eBPF upgrade)
- eBPF mode provides an upgrade path for performance-sensitive deployments
