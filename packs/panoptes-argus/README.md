# Argus FIM - File Integrity Monitoring

**Real-time file integrity monitoring for Kubernetes using inotify**

Argus is named after **Argus Panoptes**, the 100-eyed giant of Greek mythology who was the ultimate guardian. This component watches your container filesystems for changes with the vigilance of 100 eyes.

## Features

- Real-time file change detection using Linux inotify
- Container-aware monitoring with automatic PID namespace resolution
- Support for containerd and CRI-O runtimes
- Recursive directory watching with configurable depth
- Event filtering by type (create, modify, delete, access, etc.)
- Prometheus metrics for observability
- Kubernetes-native CRD-based configuration

## Installation via Spectro Cloud Palette

1. Add the Argus FIM pack to your cluster profile
2. Configure values as needed (see Configuration below)
3. Apply the profile to your cluster

## Quick Start

After installation, create an ArgusWatcher to monitor files:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: critical-files
  namespace: default
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/sudoers
      events:
        - modify
        - delete
        - create
      recursive: false
      tags:
        severity: critical
        compliance: pci-dss
```

## Configuration

### Controller Settings

| Parameter | Description | Default |
|-----------|-------------|---------|
| `controller.replicas` | Number of controller replicas | `1` |
| `controller.resources.requests.cpu` | CPU request | `100m` |
| `controller.resources.requests.memory` | Memory request | `128Mi` |
| `controller.resources.limits.cpu` | CPU limit | `500m` |
| `controller.resources.limits.memory` | Memory limit | `256Mi` |

### Daemon Settings

| Parameter | Description | Default |
|-----------|-------------|---------|
| `daemon.resources.requests.cpu` | CPU request | `50m` |
| `daemon.resources.requests.memory` | Memory request | `64Mi` |
| `daemon.resources.limits.cpu` | CPU limit | `200m` |
| `daemon.resources.limits.memory` | Memory limit | `128Mi` |
| `daemon.tolerations` | DaemonSet tolerations | `[{operator: Exists, effect: NoSchedule}]` |

### Observability

| Parameter | Description | Default |
|-----------|-------------|---------|
| `observability.prometheus.enabled` | Enable Prometheus metrics | `true` |
| `observability.prometheus.serviceMonitor.enabled` | Create ServiceMonitor | `true` |
| `observability.opentelemetry.enabled` | Enable OTLP export | `false` |
| `observability.grafana.dashboards.enabled` | Install Grafana dashboards | `true` |

## CRD Reference: ArgusWatcher

### Spec Fields

| Field | Type | Description |
|-------|------|-------------|
| `selector` | LabelSelector | Pods to monitor |
| `subjects` | []Subject | File paths and events to watch |
| `subjects[].paths` | []string | Paths to monitor |
| `subjects[].events` | []string | Events: access, modify, create, delete, open, close, move, all |
| `subjects[].recursive` | bool | Watch directories recursively |
| `subjects[].maxDepth` | int | Max recursion depth |
| `subjects[].ignore` | []string | Paths to ignore |
| `subjects[].tags` | map | Custom labels for events |
| `paused` | bool | Pause watching without deletion |

### Status Fields

| Field | Type | Description |
|-------|------|-------------|
| `observablePods` | int | Pods matching selector |
| `watchedPods` | int | Pods with active watches |
| `eventsDetected` | int | Total events since creation |
| `conditions` | []Condition | Reconciliation status |

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `argus_controller_events_detected_total` | Counter | Total file events by type |
| `argus_controller_watch_descriptors_total` | Gauge | Active inotify watches |
| `argus_controller_watched_pods_total` | Gauge | Pods with active watches |
| `argus_controller_reconcile_duration_seconds` | Histogram | Reconciliation latency |

## Security Requirements

The argusd daemon requires elevated capabilities:
- `SYS_PTRACE`: For /proc access to container PIDs (required)
- `DAC_READ_SEARCH`: For container filesystem traversal (optional)

## Requirements

- Kubernetes 1.28+
- Linux nodes (inotify kernel feature)
- containerd or CRI-O runtime

## Related Packs

- **Panoptes Security Suite**: Full suite with Argus + Janus + Dashboard
- **Janus Audit**: File access auditing and enforcement

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
