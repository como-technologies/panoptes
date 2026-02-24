# ADR-0004: fanotify for File Access Control

## Status

Accepted

## Context

Panoptes Janus needs to both audit and optionally block file access in containers. The options:

1. **fanotify** — Linux kernel API for filesystem notification and permission events (since Linux 2.6.37, permission events since 5.1)
2. **LSM (AppArmor/SELinux/BPF-LSM)** — Linux Security Modules for mandatory access control
3. **seccomp-BPF** — System call filtering at the process level
4. **eBPF LSM hooks** — BPF programs attached to LSM hooks (since Linux 5.7)

## Decision

We chose **fanotify** with permission events (`FAN_OPEN_PERM`, `FAN_ACCESS_PERM`) for Janus.

## Rationale

**Why fanotify:**

- **Path-based policies**: fanotify operates on file paths and mount points, which maps directly to how compliance frameworks specify access controls ("deny access to /etc/shadow"). LSM policies operate on security labels, which require a translation layer.
- **Dynamic policy updates**: fanotify marks can be added/removed at runtime without process restart. LSM profiles (AppArmor) require profile reload; SELinux requires policy recompilation.
- **Audit + enforce in one mechanism**: A single fanotify group can audit some paths (FAN_ACCESS) and enforce others (FAN_ACCESS_PERM with FAN_DENY). LSM provides enforcement but requires separate audit configuration.
- **Container-transparent**: fanotify watches files via `/proc/{pid}/root` mount namespace traversal — the same mechanism Argus uses. No container-specific LSM profile management needed.
- **No host-level configuration**: fanotify requires only `SYS_ADMIN` + `SYS_PTRACE` capabilities. LSM requires host-level AppArmor/SELinux configuration, which may conflict with the platform's existing security posture.

**Why not LSM:**

- LSM operates at a different abstraction level (security labels vs. file paths)
- Requires host-level configuration that conflicts with existing platform security
- AppArmor profiles are static — can't be updated dynamically from CRD changes
- KubeArmor already does LSM-based enforcement well; we don't need to duplicate it

**Why not seccomp-BPF:**

- Operates at syscall level, not file path level — too coarse for compliance-driven access control
- Can't differentiate between "deny access to /etc/shadow" and "allow access to /etc/passwd" — both use the `open` syscall
- Good for process sandboxing, not file access auditing

**Why not eBPF LSM:**

- Requires Linux 5.7+ with BPF LSM enabled — many enterprise kernels don't have this
- Higher implementation complexity than fanotify for the same file-level access control
- Better suited for comprehensive security policies, overkill for file access auditing

## Consequences

- Janus requires `SYS_ADMIN` capability for fanotify permission events (documented in `docs/security/privileged-container-justification.md`)
- fanotify has a limit on the number of marks per group (`/proc/sys/fs/fanotify/max_user_marks`)
- Permission events introduce latency on file access (kernel waits for allow/deny response) — `enforcing: false` (audit-only) mode avoids this
- fanotify cannot distinguish between read and write opens in permission mode on older kernels (pre-5.1) — documented as a known limitation
- The `AUDIT_WRITE` capability enables kernel audit log integration for additional attribution
