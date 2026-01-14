# Janus Operator

**Kubernetes-native File Access Auditing and Enforcement using Linux fanotify**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../LICENSE)
[![Go Version](https://img.shields.io/badge/Go-1.24+-blue.svg)](https://golang.org)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-1.28%2B-blue.svg)](https://kubernetes.io)

## Overview

Janus is a Kubernetes operator that provides real-time file access auditing and permission
enforcement for containerized workloads. Using Linux fanotify, Janus intercepts file access
requests at the kernel level, enabling both audit logging and active access control.

### Original Project

Janus was originally created by [ClusterGarage](https://clustergarage.io/janus/) circa 2018.
This modernized version updates the project for Kubernetes 1.28+ while preserving the core
functionality.

### Use Cases

- **Access Auditing**: Log all file access attempts for compliance and forensics
- **Runtime Protection**: Block unauthorized access to sensitive files
- **Policy Enforcement**: Implement allow/deny rules for file access
- **Compliance Evidence**: Generate audit trails for SOC2, HIPAA, PCI-DSS

## How Janus Differs from Argus

| Feature | Argus (inotify) | Janus (fanotify) |
|---------|-----------------|------------------|
| Purpose | Detect changes after they happen | Intercept access before it completes |
| Enforcement | None (notification only) | Can block access |
| Events | File modifications | File access (open, read) |
| Use case | File integrity monitoring | Access control and auditing |
| Kernel interface | inotify | fanotify |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Janus Architecture                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌────────────────────┐         ┌────────────────────────────────────┐  │
│  │   JanusGuard CR    │────────▶│         Janus Operator             │  │
│  │                    │         │  ┌──────────────────────────────┐  │  │
│  │ spec:              │         │  │     Reconciler Loop          │  │  │
│  │   selector: {...}  │         │  │  • Watch JanusGuard CRs      │  │  │
│  │   subjects:        │         │  │  • Find matching pods        │  │  │
│  │     - allow: [...] │         │  │  • Call janusd via gRPC      │  │  │
│  │       deny: [...]  │         │  │  • Update status             │  │  │
│  │   enforcing: true  │         │  └──────────────────────────────┘  │  │
│  └────────────────────┘         └───────────────┬────────────────────┘  │
│                                                 │ gRPC                  │
│                                                 ▼                       │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    janusd DaemonSet                              │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐              │   │
│  │  │ janusd  │  │ janusd  │  │ janusd  │  │ janusd  │   (per node) │   │
│  │  │ Node 1  │  │ Node 2  │  │ Node 3  │  │ Node N  │              │   │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘              │   │
│  │       │            │            │            │                   │   │
│  │       └────────────┴────────────┴────────────┘                   │   │
│  │                         │                                        │   │
│  │         fanotify_mark() with FAN_ACCESS_PERM / FAN_OPEN_PERM     │   │
│  │                         ▼                                        │   │
│  │  ┌──────────────────────────────────────────────────────────┐    │   │
│  │  │              Linux Kernel (fanotify subsystem)           │    │   │
│  │  │  • FAN_ACCESS_PERM - permission request for read         │    │   │
│  │  │  • FAN_OPEN_PERM - permission request for open           │    │   │
│  │  │  • Response: FAN_ALLOW or FAN_DENY                       │    │   │
│  │  └──────────────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    Kernel Audit Subsystem                        │   │
│  │  • Audit log entries for access events                           │   │
│  │  • Integration with auditd                                       │   │
│  │  • SIEM-ready log format                                         │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Hardening: Init Container Injection

To eliminate the race condition where file access could occur before fanotify guards
are active, Janus provides a webhook-based hardening pattern that blocks pod startup until
protection is in place.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Pod Startup Flow (Hardened)                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Pod CREATE request → Admission Webhook                      │
│  2. Webhook injects guard-wait init container                   │
│  3. Pod scheduled, init container starts                        │
│  4. guard-wait polls GetGuardState RPC                          │
│  5. Janusd creates guard SYNCHRONOUSLY (marks registered)       │
│  6. GetGuardState returns marks_registered=true                 │
│  7. guard-wait exits 0 → main containers start (PROTECTED)      │
│                                                                 │
│  Defense layers:                                                │
│  ✓ Synchronous guard init (fanotify marks before response)      │
│  ✓ Readiness fields in proto (marks_registered, ready_at)       │
│  ✓ Webhook + init container (blocks pod until ready)            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Enabling Webhook Injection

1. **Label the namespace** to enable guard injection:
   ```bash
   kubectl label namespace <namespace> janus.panoptes.io/guard-injection=enabled
   ```

2. **Create a JanusGuard** that selects pods in that namespace:
   ```yaml
   apiVersion: janus.como-technologies.io/v2
   kind: JanusGuard
   metadata:
     name: my-guard
     namespace: <namespace>
   spec:
     selector:
       matchLabels:
         app: my-app
     subjects:
       - deny: [/etc/shadow]
         events: [open]
     enforcing: true
   ```

3. **Deploy pods** - the webhook will automatically inject the `wait-for-guard` init container.

### Disabling Injection for Specific Pods

Add the annotation `janus.panoptes.io/inject: "false"` to bypass injection:

```yaml
metadata:
  annotations:
    janus.panoptes.io/inject: "false"
```

### Configuration

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `GUARD_WAIT_IMAGE` | Image for the guard-wait init container | `panoptes/guard-wait:latest` |
| `JANUSD_ADDRESS` | Address of the janusd gRPC service | `http://janusd.panoptes-system:50052` |
| `GUARD_MAX_WAIT_SECS` | Maximum time to wait for guard readiness | `30` |

### Production Deployment

The webhook requires TLS certificates and must be explicitly enabled. For complete setup instructions including:
- cert-manager integration
- Self-signed certificate generation
- MutatingWebhookConfiguration deployment
- Troubleshooting

See the comprehensive guide: [WEBHOOK_DEPLOYMENT.md](../../docs/WEBHOOK_DEPLOYMENT.md)

**Quick start (requires cert-manager):**
```bash
# Add --enable-webhook=true to operator deployment args
kubectl patch deployment janus-operator-controller-manager -n panoptes-system --type='json' \
  -p='[{"op": "add", "path": "/spec/template/spec/containers/0/args/-", "value": "--enable-webhook=true"}]'
```

## Components

### Operator (This Repository)

The Janus Operator is a Kubernetes controller built with [Kubebuilder](https://kubebuilder.io).
It watches JanusGuard custom resources and coordinates with janusd daemons to set up
file access guards.

**Kubebuilder-generated scaffolding:**
- `cmd/main.go` - Entry point with controller-runtime setup
- `internal/controller/suite_test.go` - Test suite setup
- `config/` - Kustomize manifests (CRDs, RBAC, deployment)
- `Makefile` - Standard kubebuilder targets
- `Dockerfile` - Multi-stage build

**Custom implementation:**
- `api/v2/janusguard_types.go` - v2 CRD types (current, storage version)
- `api/v1/janusguard_types.go` - v1 CRD types (deprecated)
- `api/v1/janusguard_conversion.go` - v1 ↔ v2 conversion
- `internal/controller/janusguard_controller.go` - Reconciliation logic
- `internal/grpc/client.go` - gRPC client for janusd communication

### Daemon (janusd)

The janusd daemon runs as a DaemonSet on each node. It receives guard requests from the
operator via gRPC and sets up fanotify watches on container filesystems. When access events
occur, janusd evaluates policies and responds with allow/deny decisions.

See [daemons/janusd/README.md](../../daemons/janusd/README.md) for daemon documentation.

## CRD Reference

### API Versions

| Version | Status | Description |
|---------|--------|-------------|
| **v2** | Current (recommended) | Storage version, use for new deployments |
| **v1** | Deprecated | Legacy version, kept for migration only |

### JanusGuard v2 (Recommended)

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: example-guard
  namespace: default
spec:
  # Pod selector (required)
  selector:
    matchLabels:
      app: my-app
    matchExpressions:
      - key: environment
        operator: In
        values: ["production"]

  # Guard subjects (required, max 20)
  subjects:
    - # Allowed paths (max 100 per subject)
      # Glob patterns supported
      allow:
        - /app/**
        - /tmp/**
        - /var/log/app/**

      # Denied paths (max 100 per subject)
      # Glob patterns supported
      deny:
        - /etc/shadow
        - /etc/sudoers
        - /root/**
        - /home/*/.ssh/**

      # Events to guard (required)
      # Options: access, open, all
      events:
        - open
        - access

      # Only guard directory access
      onlyDir: false

      # Allow file owner to access
      autoAllowOwner: true

      # Log to kernel audit subsystem
      audit: true

      # Custom metadata tags
      tags:
        policy: sensitive-data
        compliance: soc2

  # Container runtime: containerd, cri-o, or auto (default: auto)
  containerRuntime: auto

  # Custom log format template
  logFormat: "{{.Timestamp}} {{.Response}} {{.Path}} {{.Pid}}"

  # Temporarily disable guarding
  paused: false

  # Enforce deny rules (false = audit only, true = block access)
  enforcing: true

status:
  # Last observed spec generation
  observedGeneration: 1

  # Number of pods matching selector
  observablePods: 5

  # Number of pods with active guards
  guardedPods: 5

  # Total denied access attempts
  totalDeniedEvents: 42

  # Total audited events
  totalAuditedEvents: 15678

  # Per-pod status
  podStatuses:
    - podName: my-app-abc123
      nodeName: worker-1
      guardActive: true
      deniedEvents: 3
      auditedEvents: 1234
      lastEventTime: "2026-01-10T10:30:00Z"
      conditions:
        - type: Guarding
          status: "True"
          reason: GuardActive
          message: "fanotify guard active"

  # Standard Kubernetes conditions
  conditions:
    - type: Ready
      status: "True"
      reason: Reconciled
      message: "All pods guarded successfully"
```

### v1 to v2 Migration

The v2 API is the current recommended version. v1 is deprecated and kept only for migration.

| Feature | v1 (deprecated) | v2 (current) |
|---------|-----------------|--------------|
| Status | Deprecated | Recommended, storage version |
| Conversion | Spoke (converts to v2) | Hub (storage) |
| Use case | Upgrading from | New deployments |

Conversion webhooks automatically handle v1 ↔ v2 conversion. Existing v1 resources will continue to work but should be migrated to v2.

**Migration steps:**
1. Update your manifests to use `apiVersion: janus.como-technologies.io/v2`
2. Apply the updated manifests
3. Conversion webhooks handle the rest automatically

## Enforcement Modes

### Enforcing Mode (`spec.enforcing: true`)

In enforcing mode, Janus actively blocks access to denied paths:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  enforcing: true
  subjects:
    - deny:
        - /etc/shadow
      events: [open]
```

When a process attempts to open `/etc/shadow`, Janus returns `FAN_DENY` and the
kernel blocks the access. The process receives `EPERM` (Permission denied).

### Audit-Only Mode (`spec.enforcing: false`)

In audit-only mode, Janus logs all access events without blocking:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  enforcing: false
  subjects:
    - deny:
        - /etc/shadow
      events: [open]
      audit: true
```

Access attempts are logged but not blocked. Useful for:
- Testing policies before enforcement
- Compliance auditing without impacting applications
- Understanding access patterns

## Installation

### Prerequisites

- Kubernetes 1.28+
- containerd or CRI-O container runtime
- Linux kernel 5.x+ (for full fanotify support)
- Go 1.24+ (for building)
- Docker 17.03+ (for building images)

### Deploy with kubectl

```bash
# Install CRDs
make install

# Deploy operator
make deploy IMG=<your-registry>/janus-operator:v2.0.0
```

### Deploy with Helm

```bash
helm install janus ./charts/janus-operator \
  --namespace janus-system \
  --create-namespace
```

### Build from Source

```bash
# Generate code and manifests
make generate manifests

# Build binary
make build

# Build and push Docker image
make docker-build docker-push IMG=<your-registry>/janus-operator:v2.0.0
```

## Configuration

### Operator Flags

| Flag | Environment Variable | Default | Description |
|------|---------------------|---------|-------------|
| `--metrics-bind-address` | `METRICS_BIND_ADDRESS` | `:8080` | Metrics endpoint |
| `--health-probe-bind-address` | `HEALTH_PROBE_BIND_ADDRESS` | `:8081` | Health probes endpoint |
| `--leader-elect` | `LEADER_ELECT` | `false` | Enable leader election |
| `--janusd-port` | `JANUSD_PORT` | `50052` | gRPC port for janusd |

### RBAC

The operator requires these permissions:

```yaml
# JanusGuard resources
- apiGroups: ["janus.como-technologies.io"]
  resources: ["janusguards"]
  verbs: ["get", "list", "watch"]
- apiGroups: ["janus.como-technologies.io"]
  resources: ["janusguards/status"]
  verbs: ["get", "update", "patch"]
- apiGroups: ["janus.como-technologies.io"]
  resources: ["janusguards/finalizers"]
  verbs: ["update"]

# Pod discovery
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["get", "list", "watch"]

# Events
- apiGroups: [""]
  resources: ["events"]
  verbs: ["create", "patch"]

# Leader election
- apiGroups: ["coordination.k8s.io"]
  resources: ["leases"]
  verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
```

## Monitoring

### Prometheus Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `janus_reconcile_total` | Counter | Total reconciliations |
| `janus_reconcile_duration_seconds` | Histogram | Reconciliation duration |
| `janus_guarded_pods` | Gauge | Pods with active guards |
| `janus_active_guards` | Gauge | Total fanotify guards |
| `janus_access_events_total` | Counter | Access events by response |
| `janus_denied_access_total` | Counter | Denied access attempts |
| `janus_grpc_requests_total` | Counter | gRPC requests to daemons |
| `janus_grpc_request_duration_seconds` | Histogram | gRPC request duration |

### Health Endpoints

- `/healthz` - Liveness probe
- `/readyz` - Readiness probe
- `/metrics` - Prometheus metrics

### Alerting

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: janus-alerts
spec:
  groups:
    - name: janus.security
      rules:
        - alert: UnauthorizedAccessAttempt
          expr: |
            increase(janus_denied_access_total[5m]) > 0
          labels:
            severity: critical
          annotations:
            summary: "Unauthorized file access blocked"
            description: "{{ $value }} access attempts denied in {{ $labels.namespace }}"

        - alert: HighAccessDenialRate
          expr: |
            rate(janus_denied_access_total[5m]) > 10
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: "High rate of access denials"
```

## Examples

### Protect Sensitive System Files

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: protect-system
spec:
  selector:
    matchLabels:
      app.kubernetes.io/part-of: my-system
  enforcing: true
  subjects:
    - allow:
        - /usr/**
        - /lib/**
        - /bin/**
      deny:
        - /etc/shadow
        - /etc/sudoers
        - /etc/sudoers.d/**
        - /root/**
      events: [open, access]
      audit: true
      tags:
        severity: critical
```

### Audit Database Access

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: audit-database
spec:
  selector:
    matchLabels:
      app: postgres
  enforcing: false  # Audit only
  subjects:
    - allow:
        - /var/lib/postgresql/**
      events: [open, access]
      audit: true
      tags:
        compliance: soc2
        data-classification: sensitive
```

### Restrict Container to Specific Paths

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: app-sandbox
spec:
  selector:
    matchLabels:
      sandbox: enabled
  enforcing: true
  subjects:
    - allow:
        - /app/**
        - /tmp/**
      deny:
        - /**  # Deny everything else
      events: [open]
      autoAllowOwner: true
      tags:
        policy: sandbox
```

## Kernel Audit Integration

When `audit: true` is set, Janus writes events to the kernel audit subsystem:

```
type=FANOTIFY msg=audit(1704883800.123:456):
  operation="open"
  path="/etc/shadow"
  response="deny"
  pid=12345
  guard="protect-system"
  namespace="production"
```

These events can be collected by auditd and forwarded to SIEM systems.

## Troubleshooting

### Guard not blocking access

1. Verify `enforcing: true` is set
2. Check if path matches deny patterns (glob matching)
3. Check if `autoAllowOwner: true` is allowing owner access
4. Verify daemon logs: `kubectl logs -n janus-system -l app=janusd`

### High latency on file access

fanotify permission events add latency. Consider:
- Limit guarded paths to sensitive areas only
- Use `audit: false` for non-compliance workloads
- Monitor `janus_grpc_request_duration_seconds` metric

### Kernel audit buffer overflow

When `audit: true`, high-volume access can overflow the kernel audit buffer:

```bash
# Check for lost events
dmesg | grep audit

# Increase buffer size
auditctl -b 8192
```

### Permission denied errors in daemon

Ensure janusd has required capabilities:
- `SYS_ADMIN` - fanotify initialization and container namespace access
- `SYS_PTRACE` - Access /proc for container PIDs
- `DAC_READ_SEARCH` - Traverse container filesystems
- `AUDIT_WRITE` - Write to kernel audit log

## Development

### Run Locally

```bash
# Install CRDs
make install

# Run controller locally
make run
```

### Run Tests

```bash
# Unit tests
make test

# Integration tests (requires envtest)
make test-integration
```

### Generate Code

```bash
# After modifying types
make generate

# After modifying markers
make manifests
```

## Project Structure

```
janus-operator/
├── api/
│   ├── v2/                    # Current v2 API (hub, storage version)
│   │   ├── janusguard_types.go
│   │   ├── groupversion_info.go
│   │   └── zz_generated.deepcopy.go
│   └── v1/                    # Deprecated v1 API (spoke, for migration)
│       ├── janusguard_types.go
│       ├── janusguard_conversion.go
│       ├── groupversion_info.go
│       └── zz_generated.deepcopy.go
├── cmd/
│   └── main.go                # Entry point (kubebuilder generated)
├── config/
│   ├── crd/                   # CRD manifests
│   ├── rbac/                  # RBAC manifests
│   ├── manager/               # Operator deployment
│   ├── samples/               # Example CRs (v2 recommended)
│   └── webhook/               # Conversion webhook config
├── internal/
│   ├── controller/            # Reconciliation logic
│   │   ├── janusguard_controller.go
│   │   └── suite_test.go
│   └── grpc/                  # gRPC client for janusd
│       └── client.go
├── Dockerfile
├── Makefile
└── PROJECT                    # Kubebuilder project file
```

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.

## Acknowledgments

Based on the original [Janus project](https://clustergarage.io/janus/) by ClusterGarage.
