# Overlapping Guards and Watchers

> **Important:** Multiple JanusGuards or ArgusWatchers monitoring the same file paths results in undefined behavior. This document explains why and provides best practices.

---

## The Problem

When you create multiple JanusGuards that watch the same paths, only ONE guard will process each access event. This is a fundamental limitation of Linux fanotify.

### Example: Overlapping JanusGuards

```yaml
# Guard 1: PCI-DSS compliance
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: pci-dss-access
spec:
  subjects:
    - paths: ["/etc/shadow", "/etc/passwd"]
      deny: ["/etc/shadow"]

# Guard 2: General security
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: test-guard
spec:
  subjects:
    - paths: ["/etc/shadow"]  # OVERLAPS with pci-dss-access!
      audit: true
```

**What happens when a process accesses `/etc/shadow`:**

1. Kernel generates ONE permission event
2. Both guards have fanotify marks on this path
3. **Race condition**: First guard to respond wins
4. If `pci-dss-access` responds DENY first → access blocked, `test-guard` never sees it
5. If `test-guard` responds ALLOW first → access allowed, `pci-dss-access` never sees it

---

## Why This Happens

### fanotify Permission Model

fanotify's permission events (`FAN_OPEN_PERM`, `FAN_ACCESS_PERM`) are designed for single-decider access control:

```
┌─────────────────────────────────────────────────────────────┐
│  Process: cat /etc/shadow                                   │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  Kernel generates permission request                        │
│  Waits for response from ANY fanotify group                 │
└─────────────────────────────────────────────────────────────┘
                           │
            ┌──────────────┴──────────────┐
            ▼                             ▼
    pci-dss-access                   test-guard
    (janusd session 1)               (janusd session 2)
            │                             │
            │ Responds DENY               │ Still processing...
            ▼                             ▼
    ─────────────────────────────────────────────────────────
            │
            ▼
    Access DENIED
    (test-guard's response is discarded)
```

This is by design—the kernel cannot wait for multiple deciders because:
- It would create deadlock risks
- There's no standard for resolving conflicting decisions
- Performance would suffer

### inotify Behavior (ArgusWatchers)

inotify (used by Argus) is different—it's notification-only, not permission-based. Multiple watchers CAN all receive the same event. However, the current implementation attributes events to only one watcher based on which session processed it first.

---

## Impact

| Issue | JanusGuard (fanotify) | ArgusWatcher (inotify) |
|-------|----------------------|------------------------|
| Race condition on decision | YES - First responder wins | N/A - No decisions |
| Missing events | YES - Loser never sees event | NO - All can see |
| Wrong attribution | YES - Only winner is logged | YES - Only first match logged |
| Policy conflicts | YES - Unpredictable outcome | N/A |
| Wasted resources | Moderate | YES - Duplicate watches |

---

## Best Practices

### 1. Use Non-Overlapping Paths

Design your guards/watchers to monitor distinct paths:

```yaml
# Good: Non-overlapping guards
---
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: credential-guard
spec:
  subjects:
    - paths: ["/etc/shadow", "/etc/gshadow"]
---
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: config-guard
spec:
  subjects:
    - paths: ["/etc/ssh/", "/etc/sudoers.d/"]  # Different paths
```

### 2. Use Label Selectors for Separation

Use different selectors so guards apply to different pods:

```yaml
# Guard for payment pods
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: pci-guard
spec:
  selector:
    matchLabels:
      pci-scope: "true"  # Only payment pods
  subjects:
    - paths: ["/etc/shadow"]
      deny: ["/etc/shadow"]

# Guard for general pods (excludes PCI pods)
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: general-guard
spec:
  selector:
    matchExpressions:
      - key: pci-scope
        operator: NotIn
        values: ["true"]  # Everything except payment pods
  subjects:
    - paths: ["/etc/shadow"]
      audit: true
```

### 3. Use a Single Comprehensive Guard

Instead of multiple specialized guards, use one guard with all rules:

```yaml
# Single guard with all policies
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: security-guard
spec:
  subjects:
    # PCI requirement: block shadow access
    - paths: ["/etc/shadow"]
      deny: ["/etc/shadow"]
      tags:
        compliance: pci-dss
    # Audit other credential files
    - paths: ["/etc/passwd", "/etc/group"]
      audit: true
      tags:
        compliance: general
    # Monitor SSH configs
    - paths: ["/etc/ssh/"]
      audit: true
      tags:
        category: ssh
```

### 4. Use Namespaces for Isolation

Deploy guards in different namespaces with non-overlapping selectors:

```yaml
# In namespace: pci-workloads
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: pci-guard
  namespace: pci-workloads
spec:
  selector:
    matchLabels:
      app: payment-api  # Only matches pods in this namespace

# In namespace: general-workloads
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: general-guard
  namespace: general-workloads
spec:
  selector:
    matchLabels:
      monitored: "true"  # Only matches pods in this namespace
```

---

## Detecting Overlaps

Currently, Panoptes does not automatically detect overlapping guards. To check manually:

```bash
# List all guards and their paths
kubectl get jg -A -o jsonpath='{range .items[*]}{.metadata.name}: {.spec.subjects[*].paths}{"\n"}{end}'

# Look for duplicate paths across guards
```

**Future enhancement:** The operator may add validation warnings when guards overlap. See [FUTURE_STATE.md](../FUTURE_STATE.md#12-multi-guard-policy-consolidation) for planned improvements.

---

## Related Documentation

- [JanusGuard API Reference](../api/janusguard-v2.md)
- [ArgusWatcher API Reference](../api/arguswatcher-v2.md)
- [Future State: Multi-Guard Policy](../FUTURE_STATE.md#12-multi-guard-policy-consolidation)
