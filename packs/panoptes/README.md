# Panoptes Security Suite

**All-seeing Kubernetes file integrity and access monitoring**

Panoptes is a comprehensive security suite that combines:
- **Argus**: File Integrity Monitoring (FIM) using inotify
- **Janus**: File Access Auditing using fanotify
- **Panoptes Eye**: Real-time security monitoring dashboard

## Greek Mythology Origin

The name **Panoptes** (Greek: Πανόπτης, "all-seeing") derives from **Argus Panoptes**, the 100-eyed giant of Greek mythology who was the ultimate guardian. Combined with **Janus**, the two-faced Roman god who could see the past and future, this suite provides complete visibility into file system security.

## Installation via Spectro Cloud Palette

1. Add the Panoptes pack to your cluster profile
2. Select a preset:
   - **default**: Full suite with all components
   - **compliance**: Enhanced for PCI-DSS/SOC2/HIPAA
   - **minimal**: Argus FIM only
3. Configure values as needed
4. Apply the profile to your cluster

## Components

### Argus (File Integrity Monitoring)

Monitors file system changes in container filesystems using inotify.

**CRD**: `ArgusWatcher`
```yaml
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: critical-files
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
      events:
        - modify
        - delete
      recursive: false
```

### Janus (File Access Auditing)

Audits and controls file access in containers using fanotify.

**CRD**: `JanusGuard`
```yaml
apiVersion: janus.como-technologies.io/v1
kind: JanusGuard
metadata:
  name: sensitive-access
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - deny:
        - /etc/shadow
      events:
        - access
        - open
      audit: true
      enforcing: true  # Set to false for dry-run mode
```

### Panoptes Eye (Dashboard)

Web-based monitoring interface for:
- Real-time event visualization
- ArgusWatcher/JanusGuard management
- File tree browser
- Compliance reporting

## Configuration

### Values Reference

| Parameter | Description | Default |
|-----------|-------------|---------|
| `components.argus.enabled` | Enable Argus FIM | `true` |
| `components.janus.enabled` | Enable Janus Audit | `true` |
| `components.dashboard.enabled` | Enable Panoptes Eye | `true` |
| `global.cluster.name` | Cluster identifier for multi-cluster | `""` |
| `observability.prometheus.enabled` | Enable Prometheus metrics | `true` |
| `dashboard.ingress.enabled` | Enable Ingress for dashboard | `false` |

### Multi-Cluster Deployment

For Palette-managed multi-cluster deployments, set cluster identifiers:

```yaml
global:
  cluster:
    name: "prod-east-1"
    environment: "production"
    region: "us-east-1"
```

All metrics and logs will include these labels for cross-cluster queries.

## Presets

### Default
Standard production deployment with all components and observability.

### Compliance
Enhanced configuration for PCI-DSS, SOC2, HIPAA:
- Increased resource limits
- More frequent metric scraping
- Lower alert thresholds
- Full audit logging

### Minimal
Lightweight FIM-only deployment:
- Argus only (no Janus or dashboard)
- Minimal resources
- Basic Prometheus metrics

## Requirements

- Kubernetes 1.28+
- Linux nodes (inotify/fanotify kernel features)
- Prometheus (optional, for metrics)
- Ingress controller (optional, for dashboard access)

## Security

The daemons require elevated capabilities:
- `SYS_ADMIN`: For inotify/fanotify initialization
- `SYS_PTRACE`: For container PID namespace access
- `DAC_READ_SEARCH`: For container filesystem access
- `AUDIT_WRITE`: For kernel audit log (Janus only)

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
