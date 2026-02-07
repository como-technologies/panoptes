# Janus Audit - File Access Auditing

**Real-time file access auditing and enforcement for Kubernetes using fanotify**

Janus is named after the **Roman god Janus**, the two-faced deity who could see both past and future. This component audits file access attempts and can enforce access policies, giving you visibility into both what has been accessed and what will be blocked.

## Features

- Real-time file access auditing using Linux fanotify
- Allow/deny policy enforcement at the kernel level
- Container-aware monitoring with automatic PID namespace resolution
- Support for containerd and CRI-O runtimes
- Dry-run mode for policy testing
- Kernel audit log integration
- Prometheus metrics for observability
- Kubernetes-native CRD-based configuration

## Installation via Spectro Cloud Palette

1. Add the Janus Audit pack to your cluster profile
2. Configure values as needed (see Configuration below)
3. Apply the profile to your cluster

## Quick Start

After installation, create a JanusGuard to audit/enforce file access:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: sensitive-files
  namespace: default
spec:
  selector:
    matchLabels:
      app: my-app
  enforcing: true  # Set to false for dry-run mode
  subjects:
    - deny:
        - /etc/shadow
        - /etc/gshadow
      events:
        - access
        - open
      audit: true
      defaultResponse: deny
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

### Enforcement Settings

| Parameter | Description | Default |
|-----------|-------------|---------|
| `enforcement.enabled` | Global enforcement mode | `true` |
| `audit.kernelAudit` | Write to kernel audit log | `true` |

### Observability

| Parameter | Description | Default |
|-----------|-------------|---------|
| `observability.prometheus.enabled` | Enable Prometheus metrics | `true` |
| `observability.prometheus.serviceMonitor.enabled` | Create ServiceMonitor | `true` |
| `observability.opentelemetry.enabled` | Enable OTLP export | `false` |
| `observability.grafana.dashboards.enabled` | Install Grafana dashboards | `true` |

## CRD Reference: JanusGuard

### Spec Fields

| Field | Type | Description |
|-------|------|-------------|
| `selector` | LabelSelector | Pods to monitor |
| `subjects` | []Subject | Access policies |
| `subjects[].allow` | []string | Paths to allow access |
| `subjects[].deny` | []string | Paths to deny access |
| `subjects[].events` | []string | Events: access, open, execute, close, all |
| `subjects[].audit` | bool | Log access attempts |
| `subjects[].defaultResponse` | string | Action when no rule matches: allow, deny, audit |
| `subjects[].autoAllowOwner` | bool | Allow file owner access |
| `subjects[].tags` | map | Custom labels for events |
| `paused` | bool | Pause guarding without deletion |
| `enforcing` | bool | Block denied access (false = dry-run) |

### Status Fields

| Field | Type | Description |
|-------|------|-------------|
| `observablePods` | int | Pods matching selector |
| `guardedPods` | int | Pods with active guards |
| `totalDeniedEvents` | int | Total denied access attempts |
| `conditions` | []Condition | Reconciliation status |

## Enforcement Modes

### Enforcing Mode (`enforcing: true`)
- Access attempts to denied paths are blocked
- Events are logged to audit log
- Application receives EPERM error

### Dry-Run Mode (`enforcing: false`)
- Access attempts are logged but not blocked
- Use for policy testing before enforcement
- No impact on application behavior

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `janus_controller_denied_access_total` | Counter | Denied access attempts |
| `janus_controller_allowed_access_total` | Counter | Allowed access events |
| `janus_controller_audited_access_total` | Counter | Audited access events |
| `janus_controller_guarded_pods_total` | Gauge | Pods with active guards |
| `janus_controller_reconcile_duration_seconds` | Histogram | Reconciliation latency |

## Security Requirements

The janusd daemon requires elevated capabilities:
- `SYS_ADMIN`: For fanotify initialization
- `SYS_PTRACE`: For /proc access to container PIDs
- `DAC_READ_SEARCH`: For container filesystem traversal
- `AUDIT_WRITE`: For kernel audit log integration

## Requirements

- Kubernetes 1.28+
- Linux nodes (fanotify kernel feature)
- containerd or CRI-O runtime
- Kernel 5.1+ recommended for full fanotify features

## Related Packs

- **Panoptes Security Suite**: Full suite with Argus + Janus + Dashboard
- **Argus FIM**: File integrity monitoring

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
