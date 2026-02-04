# JanusGuard v2 API Reference

> **Note:** This document is auto-generated from Go source code comments.
> Regenerate with: `cd operators/janus-operator && make docs`

## Resource: JanusGuard

**Group:** `janus.como-technologies.io`
**Version:** `v2`
**Kind:** `JanusGuard`
**Short Name:** `jg`
**Scope:** Namespaced

JanusGuard defines file access auditing and enforcement rules using Linux fanotify for pods matching a label selector. Unlike ArgusWatcher (which detects changes), JanusGuard can actively block file access when `spec.enforcing` is true.

Requires: janusd DaemonSet with `CAP_SYS_ADMIN` (fanotify), `CAP_SYS_PTRACE`, `CAP_DAC_READ_SEARCH`.

### Quick Example

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: block-runtime-sockets
spec:
  enforcing: true
  selector:
    matchLabels:
      app: my-app
  subjects:
    - deny: ["/var/run/docker.sock", "/run/containerd/containerd.sock"]
      events: [open, access]
      tags:
        severity: critical
        compliance: cis-kubernetes
```

---

## Spec Fields

### JanusGuardSpec

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `selector` | [LabelSelector](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#labelselector-v1-meta) | Yes | - | Label selector for pods to guard. |
| `subjects` | [][JanusGuardSubject](#janusguardsubject) | Yes (min: 1, max: 20) | - | Access control rules. Each subject defines independent allow/deny rules. |
| `containerRuntime` | string | No | `auto` | Container runtime. Enum: `containerd`, `cri-o`, `auto`. |
| `logFormat` | string | No | - | Custom Go template for log output. Max length: 1024. |
| `paused` | bool | No | `false` | Temporarily suspends all access control. Removes all fanotify marks. |
| `enforcing` | bool | No | `true` | When true, deny rules block access (EACCES). When false, denials are logged but access is permitted (dry-run). |

### JanusGuardSubject

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `allow` | []string | No (max: 100) | - | Paths to explicitly allow. Evaluated after deny rules; deny wins on conflict. |
| `deny` | []string | No (max: 100) | - | Paths to block access to. Takes precedence over allow. |
| `events` | [][JanusEvent](#janusevent) | Yes (min: 1) | - | fanotify event types to monitor. |
| `onlyDir` | bool | No | `false` | Restrict marks to directories only. |
| `autoAllowOwner` | bool | No | `false` | Auto-allow when process UID matches file owner UID. |
| `audit` | bool | No | `false` | Write events to kernel audit log (requires AUDIT_WRITE). |
| `defaultResponse` | string | No | `audit` | Action for unmatched access. Enum: `allow`, `deny`, `audit`. |
| `tags` | map[string]string | No | - | Custom metadata attached to events (max 20 keys). |

### JanusEvent

| Value | Description | fanotify Flag |
|-------|-------------|--------------|
| `access` | File read | `FAN_ACCESS_PERM` (enforcing) / `FAN_ACCESS` (audit) |
| `open` | File opened | `FAN_OPEN_PERM` (enforcing) / `FAN_OPEN` (audit) |
| `execute` | File executed (kernel 5.0+) | `FAN_OPEN_EXEC_PERM` |
| `close` | File descriptor closed | `FAN_CLOSE` |
| `all` | All event types (high volume) | All flags |

### JanusResponse

| Value | Description |
|-------|-------------|
| `allow` | Permit access and record the event. |
| `deny` | Block access with EACCES. Only enforced when `spec.enforcing` is true. |
| `audit` | Record the event without affecting access. |

---

## Status Fields

### JanusGuardStatus

| Field | Type | Description |
|-------|------|-------------|
| `observedGeneration` | int64 | Most recent generation observed by the controller. |
| `observablePods` | int32 | Number of pods matching the selector. |
| `guardedPods` | int32 | Number of pods currently being guarded. |
| `marksRegistered` | bool | All fanotify marks are registered. |
| `readyAt` | [Time](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#time-v1-meta) | When the guard became fully ready. |
| `totalMountCount` | int32 | Total container mounts being guarded. |
| `totalDeniedEvents` | int64 | Total denied access attempts since creation. |
| `totalAllowedEvents` | int64 | Total allowed access events since creation. |
| `totalAuditedEvents` | int64 | Total audit-only events since creation. |
| `lastReconcileTime` | [Time](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#time-v1-meta) | When the controller last reconciled this resource. |
| `podStatuses` | [][GuardedPodStatus](#guardedpodstatus) | Detailed status per guarded pod. |
| `conditions` | [][Condition](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#condition-v1-meta) | Standard conditions: Available, Progressing, Degraded, Stalled. |

### GuardedPodStatus

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Pod name. |
| `namespace` | string | Pod namespace. |
| `nodeName` | string | Node where the pod is running. |
| `deniedCount` | int64 | Denied access attempts for this pod. |
| `allowedCount` | int64 | Allowed access events for this pod. |
| `marksRegistered` | bool | fanotify marks are active for this pod. |
| `readyAt` | Time | When guards became ready for this pod. |
| `mountCount` | int32 | Container mounts with active marks. |
| `lastDenialTime` | Time | When the last denial occurred. |

---

## Print Columns (kubectl get jg)

| Column | JSONPath | Description |
|--------|----------|-------------|
| Observable | `.status.observablePods` | Number of pods matching selector |
| Guarded | `.status.guardedPods` | Number of pods being guarded |
| Denied | `.status.totalDeniedEvents` | Total denied access attempts |
| Ready | `.status.marksRegistered` | All marks registered |
| Enforcing | `.spec.enforcing` | Whether denials are enforced |
| Age | `.metadata.creationTimestamp` | Resource age |

---

## Conditions

| Type | Description |
|------|-------------|
| `Available` | True when at least one pod is being actively guarded. |
| `Progressing` | True during reconciliation. False when reconciliation is complete. |
| `Degraded` | True when sync errors occur (daemon unreachable, mark creation failures). |
| `Stalled` | True when observable pods exist but no daemon pods are reachable. |
