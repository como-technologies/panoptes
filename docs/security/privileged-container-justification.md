# Privileged Container Justification

> **Purpose:** Explains why Panoptes daemons require elevated privileges and how the security risk is minimized.
> **Audience:** Security teams, compliance auditors, platform engineers

---

## Executive Summary

Panoptes daemons (argusd, janusd) require specific Linux capabilities to perform file integrity monitoring and file access auditing. This document explains:

1. **Why** each capability is needed
2. **What** the daemons can and cannot do with these capabilities
3. **How** the attack surface is minimized through hardening

**Key point:** The daemons use the minimum required capabilities, not full privileged mode, and are hardened with multiple defense-in-depth measures.

---

## Required Capabilities

| Capability | Daemon | Purpose |
|------------|--------|---------|
| `CAP_SYS_ADMIN` | janusd | Required for fanotify with `FAN_CLASS_CONTENT` (permission events) |
| `CAP_SYS_PTRACE` | both | Access container filesystems via `/proc/{pid}/root` |
| `CAP_DAC_READ_SEARCH` | both | Read files regardless of ownership for monitoring |

### Why Not Just `privileged: true`?

Running with `privileged: true` grants ALL capabilities (~40+). Panoptes only uses 3 specific capabilities, significantly reducing the attack surface:

```yaml
# What Panoptes uses (minimal)
securityContext:
  capabilities:
    add:
      - SYS_ADMIN
      - SYS_PTRACE
      - DAC_READ_SEARCH

# What privileged: true grants (excessive)
# ALL of: CAP_CHOWN, CAP_DAC_OVERRIDE, CAP_FOWNER, CAP_FSETID,
# CAP_KILL, CAP_SETGID, CAP_SETUID, CAP_SETPCAP, CAP_LINUX_IMMUTABLE,
# CAP_NET_BIND_SERVICE, CAP_NET_BROADCAST, CAP_NET_ADMIN, CAP_NET_RAW,
# CAP_IPC_LOCK, CAP_IPC_OWNER, CAP_SYS_MODULE, CAP_SYS_RAWIO,
# CAP_SYS_CHROOT, CAP_SYS_PTRACE, CAP_SYS_PACCT, CAP_SYS_ADMIN,
# CAP_SYS_BOOT, CAP_SYS_NICE, CAP_SYS_RESOURCE, CAP_SYS_TIME,
# CAP_SYS_TTY_CONFIG, CAP_MKNOD, CAP_LEASE, CAP_AUDIT_WRITE,
# CAP_AUDIT_CONTROL, CAP_SETFCAP, CAP_MAC_OVERRIDE, CAP_MAC_ADMIN,
# CAP_SYSLOG, CAP_WAKE_ALARM, CAP_BLOCK_SUSPEND, CAP_AUDIT_READ, ...
```

---

## Detailed Capability Justification

### CAP_SYS_ADMIN (janusd only)

**Why required:**

fanotify with permission events (`FAN_CLASS_CONTENT`) allows blocking file access. This requires `CAP_SYS_ADMIN` because it can affect system-wide behavior.

```rust
// From daemons/janusd/src/guard.rs
// FAN_CLASS_CONTENT requires CAP_SYS_ADMIN
let fd = fanotify_init(
    FAN_CLASS_CONTENT | FAN_UNLIMITED_QUEUE | FAN_UNLIMITED_MARKS,
    O_RDONLY | O_CLOEXEC,
)?;
```

**What it enables:**
- Receive fanotify permission events
- Block or allow file operations
- Monitor file access across the system

**What it does NOT enable in Panoptes context:**
- No ability to mount filesystems (no mount syscalls in code)
- No ability to load kernel modules (no module syscalls in code)
- No ability to change system time (no time syscalls in code)
- No network namespace manipulation (no netns syscalls in code)

### CAP_SYS_PTRACE

**Why required:**

To monitor files inside containers, the daemon must access the container's filesystem via `/proc/{pid}/root`. This requires `CAP_SYS_PTRACE` to traverse the symlink.

```rust
// From daemons/common/src/container_runtime.rs
// Accessing container filesystem requires SYS_PTRACE
let container_root = format!("/proc/{}/root", container_pid);
let target_path = format!("{}{}", container_root, monitored_path);
```

**What it enables:**
- Read `/proc/{pid}/root` symlink for any process
- Access container filesystems for monitoring

**What it does NOT enable in Panoptes context:**
- No actual ptrace attachment (no ptrace syscalls in code)
- No process debugging or injection
- No memory inspection of other processes

### CAP_DAC_READ_SEARCH

**Why required:**

The daemon must read files to calculate checksums for integrity verification, regardless of the file's ownership or permissions.

```rust
// From daemons/argusd/src/integrity.rs
// Reading file for checksum requires bypassing DAC
let content = std::fs::read(&file_path)?;
let hash = sha256::digest(&content);
```

**What it enables:**
- Read any file on the filesystem for integrity checks
- Traverse any directory for monitoring

**What it does NOT enable in Panoptes context:**
- No ability to write or modify files
- No ability to delete files
- No ability to change permissions

---

## What the Daemons CANNOT Do

Even with the granted capabilities, the Panoptes daemons are **code-constrained** to only perform monitoring operations:

| Action | Capability Required | In Panoptes Code? |
|--------|--------------------|--------------------|
| Read files | DAC_READ_SEARCH | Yes - for integrity checks |
| Write files | DAC_OVERRIDE | **No** |
| Delete files | DAC_OVERRIDE | **No** |
| Mount filesystems | SYS_ADMIN | **No** |
| Load kernel modules | SYS_MODULE | **No** (not granted) |
| Modify network | NET_ADMIN | **No** (not granted) |
| Create devices | MKNOD | **No** (not granted) |
| Change ownership | CHOWN | **No** (not granted) |
| Execute arbitrary code | N/A | **No** - read-only daemon |

### Code Verification

The daemon code can be audited to verify it only performs:
1. **inotify/fanotify operations** - watching files
2. **File reads** - for checksums
3. **gRPC communication** - to report events
4. **Prometheus metrics** - for observability

No syscalls for write, delete, mount, module load, or network configuration exist in the codebase.

---

## Security Hardening Measures

### 1. Read-Only Root Filesystem

```yaml
securityContext:
  readOnlyRootFilesystem: true
```

Prevents the daemon from writing anywhere except explicitly mounted volumes.

### 2. Non-Root User (where possible)

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 65534  # nobody
```

Note: Some operations require root UID. In those cases, capabilities are still limited.

### 3. No New Privileges

```yaml
securityContext:
  allowPrivilegeEscalation: false
```

Prevents the daemon from gaining additional privileges after startup.

### 4. Dropped Capabilities

```yaml
securityContext:
  capabilities:
    drop:
      - ALL
    add:
      - SYS_ADMIN
      - SYS_PTRACE
      - DAC_READ_SEARCH
```

Explicitly drop all capabilities, then add only what's needed.

### 5. Network Policy Isolation

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: panoptes-daemon-egress
spec:
  podSelector:
    matchLabels:
      app.kubernetes.io/name: argusd
  policyTypes:
    - Egress
  egress:
    # Only allow gRPC to operator
    - to:
        - namespaceSelector:
            matchLabels:
              name: panoptes-system
      ports:
        - port: 50051
```

Daemons have no network egress except to the operator.

### 6. Seccomp Profile

```yaml
securityContext:
  seccompProfile:
    type: RuntimeDefault
```

Restricts syscalls to a safe subset. A custom Panoptes seccomp profile can further restrict to only:
- `read`, `write` (to pipes/sockets)
- `openat`, `close`
- `inotify_init`, `inotify_add_watch`, `inotify_rm_watch`
- `fanotify_init`, `fanotify_mark`
- `epoll_*` (for async I/O)
- `futex` (for threading)
- `clock_gettime` (for timestamps)

### 7. AppArmor/SELinux Profile

For high-security environments, a custom AppArmor profile:

```
# /etc/apparmor.d/panoptes-daemon
profile panoptes-daemon flags=(attach_disconnected) {
  # Read-only access to system
  / r,
  /** r,

  # Required for monitoring
  /proc/*/root r,
  /proc/*/ns/* r,

  # Deny all write operations
  deny /** w,
  deny /proc/** w,
  deny /sys/** w,

  # Deny network (except unix sockets for gRPC)
  deny network inet,
  deny network inet6,
  network unix,
}
```

### 8. Resource Limits

```yaml
resources:
  limits:
    cpu: "500m"
    memory: "256Mi"
  requests:
    cpu: "100m"
    memory: "128Mi"
```

Prevents resource exhaustion attacks.

---

## Comparison to Alternatives

### Why Not eBPF-Only?

| Approach | Pros | Cons |
|----------|------|------|
| inotify/fanotify | Works on all kernels, stable, well-understood | Requires CAP_SYS_ADMIN for permission events |
| eBPF | In-kernel, lower overhead, atomic attribution | Requires CONFIG_BPF_LSM, kernel 5.7+, still needs CAP_BPF |
| Audit subsystem | Built into kernel, no caps needed for reading | Cannot block operations, only audit |

**Current strategy:** Use inotify/fanotify for broad compatibility, with eBPF as a future enhancement for environments that support it.

### Why Not Run as Sidecar?

Sidecars run in the same pod as the monitored workload:
- **Pro:** No privileged container on host
- **Con:** Cannot monitor the host filesystem
- **Con:** Cannot detect container escape attempts
- **Con:** Must be injected into every pod

Panoptes runs as a DaemonSet to monitor all containers on a node from a single point.

---

## Audit Checklist

For security teams reviewing Panoptes deployment:

- [ ] Verify daemons use specific capabilities, not `privileged: true`
- [ ] Verify `readOnlyRootFilesystem: true`
- [ ] Verify `allowPrivilegeEscalation: false`
- [ ] Verify network policies restrict egress
- [ ] Review seccomp/AppArmor profiles if deployed
- [ ] Review source code for syscall usage (see rust-security-practices.md)
- [ ] Verify container images are signed (see cryptographic-guarantees.md)

---

## References

- [Linux Capabilities man page](https://man7.org/linux/man-pages/man7/capabilities.7.html)
- [fanotify man page](https://man7.org/linux/man-pages/man7/fanotify.7.html)
- [inotify man page](https://man7.org/linux/man-pages/man7/inotify.7.html)
- [Kubernetes Security Context](https://kubernetes.io/docs/tasks/configure-pod-container/security-context/)
- [CIS Kubernetes Benchmark](https://www.cisecurity.org/benchmark/kubernetes)
