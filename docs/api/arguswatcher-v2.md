# ArgusWatcher v2 API Reference

> **Note:** This document is auto-generated from Go source code comments.
> Regenerate with: `cd operators/argus-operator && make docs`

## Resource: ArgusWatcher

**Group:** `argus.como-technologies.io`
**Version:** `v2`
**Kind:** `ArgusWatcher`
**Short Name:** `aw`
**Scope:** Namespaced

ArgusWatcher defines file integrity monitoring (FIM) rules using Linux inotify for pods matching a label selector. The argus-operator watches ArgusWatcher resources and instructs the argusd daemon on each node to create kernel-level file watches.

Requires: argusd DaemonSet with `CAP_SYS_PTRACE` and `CAP_DAC_READ_SEARCH` capabilities.

### Quick Example

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: critical-files
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - paths: ["/etc/passwd", "/etc/shadow"]
      events: [create, modify, delete]
      tags:
        severity: critical
```

---

## Spec Fields

### ArgusWatcherSpec

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `selector` | [LabelSelector](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#labelselector-v1-meta) | Yes | - | Label selector for pods to watch. Only pods matching this selector will have inotify watches created. |
| `subjects` | [][ArgusWatcherSubject](#arguswatchersubject) | Yes (min: 1, max: 20) | - | List of monitoring rules. Each subject defines independent paths, events, and tags. |
| `containerRuntime` | string | No | `auto` | Container runtime for PID detection. Enum: `containerd`, `cri-o`, `auto`. |
| `logFormat` | string | No | - | Custom Go template for log output. Max length: 1024. |
| `paused` | bool | No | `false` | Temporarily suspends all monitoring. Removes all inotify watches when true. |
| `maxWatchesPerPod` | int32 | No | `0` (unlimited) | Per-pod inotify watch limit. Prevents a single watcher from consuming all kernel resources. |

### ArgusWatcherSubject

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `paths` | []string | Yes (min: 1, max: 100) | - | File or directory paths to monitor. Supports glob patterns. |
| `events` | [][ArgusEvent](#argusevent) | Yes (min: 1) | - | inotify event types to watch for. |
| `recursive` | bool | No | `false` | Monitor subdirectories recursively. Increases watch descriptor usage. |
| `maxDepth` | int32 | No | `0` (unlimited) | Maximum recursion depth when recursive is true. |
| `onlyDir` | bool | No | `false` | Only watch directories, not individual files. |
| `tags` | map[string]string | No | - | Custom metadata attached to events (max 20 keys). |
| `skipIfMissing` | bool | No | `false` | Skip paths that don't exist instead of using proxy watches. When false, the daemon watches the nearest ancestor directory and promotes to a direct watch when the target appears. |

### ArgusEvent

| Value | Description | inotify Flag |
|-------|-------------|-------------|
| `create` | File or directory created | `IN_CREATE` |
| `modify` | File content modified | `IN_MODIFY` |
| `delete` | File or directory deleted | `IN_DELETE` |
| `moved_from` | File moved out of watched directory | `IN_MOVED_FROM` |
| `moved_to` | File moved into watched directory | `IN_MOVED_TO` |
| `attrib` | Metadata changed (permissions, ownership, timestamps) | `IN_ATTRIB` |
| `all` | All event types (high volume) | All flags |

---

## Status Fields

### ArgusWatcherStatus

| Field | Type | Description |
|-------|------|-------------|
| `observedGeneration` | int64 | Most recent generation observed by the controller. |
| `observablePods` | int32 | Number of pods matching the selector. |
| `watchedPods` | int32 | Number of pods currently being watched. |
| `totalWatchDescriptors` | int32 | Total inotify watch descriptors in use. |
| `watchesReady` | bool | All inotify watches are registered and active. |
| `readyAt` | [Time](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#time-v1-meta) | When the watcher became fully ready. |
| `eventsDetected` | int64 | Total file events detected since creation. |
| `lastReconcileTime` | [Time](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#time-v1-meta) | When the controller last reconciled this resource. |
| `podStatuses` | [][WatchedPodStatus](#watchedpodstatus) | Detailed status per watched pod. |
| `conditions` | [][Condition](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.28/#condition-v1-meta) | Standard conditions: Available, Progressing, Degraded, Stalled. |

### WatchedPodStatus

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Pod name. |
| `namespace` | string | Pod namespace. |
| `nodeName` | string | Node where the pod is running. |
| `watchDescriptors` | int32 | Number of inotify descriptors for this pod. |
| `watchesRegistered` | bool | All watches are active for this pod. |
| `readyAt` | Time | When watches became ready for this pod. |
| `lastEventTime` | Time | When the last file event was detected for this pod. |

---

## Print Columns (kubectl get aw)

| Column | JSONPath | Description |
|--------|----------|-------------|
| Observable | `.status.observablePods` | Number of pods matching selector |
| Watched | `.status.watchedPods` | Number of pods being watched |
| Events | `.status.eventsDetected` | Total events detected |
| Ready | `.status.watchesReady` | All watches registered |
| Paused | `.spec.paused` | Whether watching is paused |
| Age | `.metadata.creationTimestamp` | Resource age |

---

## Conditions

| Type | Description |
|------|-------------|
| `Available` | True when at least one pod is being actively watched. |
| `Progressing` | True during reconciliation. False when reconciliation is complete. |
| `Degraded` | True when sync errors occur (daemon unreachable, watch creation failures). |
| `Stalled` | True when observable pods exist but no daemon pods are reachable. |
