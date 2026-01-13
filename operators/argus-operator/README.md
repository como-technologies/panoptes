# Argus Operator

**Kubernetes-native File Integrity Monitoring using Linux inotify**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../../LICENSE)
[![Go Version](https://img.shields.io/badge/Go-1.22+-blue.svg)](https://golang.org)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-1.28%2B-blue.svg)](https://kubernetes.io)

## Overview

Argus is a Kubernetes operator that provides real-time file integrity monitoring (FIM) for
containerized workloads. Using Linux inotify, Argus detects file system changes at the kernel
level and reports them through the Kubernetes API.

### Original Project

Argus was originally created by [ClusterGarage](https://clustergarage.io/argus/) circa 2018.
This modernized version updates the project for Kubernetes 1.28+ while preserving the core
functionality.

### Use Cases

- **Compliance Monitoring**: Satisfy PCI-DSS 10.5.5 requirements for file integrity monitoring
- **Security Detection**: Detect unauthorized modifications to critical system files
- **Configuration Drift**: Monitor configuration files for unexpected changes
- **Audit Logging**: Generate audit trails for file system activity

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Argus Architecture                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────┐        ┌────────────────────────────────────┐  │
│  │   ArgusWatcher CR   │───────▶│         Argus Operator             │  │
│  │                     │        │  ┌──────────────────────────────┐  │  │
│  │ spec:               │        │  │     Reconciler Loop          │  │  │
│  │   selector: {...}   │        │  │  • Watch ArgusWatcher CRs    │  │  │
│  │   subjects:         │        │  │  • Find matching pods        │  │  │
│  │     - paths: [...]  │        │  │  • Call argusd via gRPC      │  │  │
│  │       events: [...] │        │  │  • Update status             │  │  │
│  └─────────────────────┘        │  └──────────────────────────────┘  │  │
│                                 └───────────────┬────────────────────┘  │
│                                                 │ gRPC                  │
│                                                 ▼                       │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    argusd DaemonSet                              │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐              │   │
│  │  │ argusd  │  │ argusd  │  │ argusd  │  │ argusd  │   (per node) │   │
│  │  │ Node 1  │  │ Node 2  │  │ Node 3  │  │ Node N  │              │   │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘              │   │
│  │       │            │            │            │                   │   │
│  │       └────────────┴────────────┴────────────┘                   │   │
│  │                         │                                        │   │
│  │                    inotify_add_watch()                           │   │
│  │                         ▼                                        │   │
│  │  ┌──────────────────────────────────────────────────────────┐    │   │
│  │  │              Linux Kernel (inotify subsystem)            │    │   │
│  │  │  • IN_ACCESS, IN_MODIFY, IN_CREATE, IN_DELETE, etc.      │    │   │
│  │  └──────────────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Hardening: Init Container Injection

To eliminate the race condition where file modifications could occur before inotify watches
are active, Argus provides a webhook-based hardening pattern that blocks pod startup until
protection is in place.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Pod Startup Flow (Hardened)                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Pod CREATE request → Admission Webhook                       │
│  2. Webhook injects watcher-wait init container                  │
│  3. Pod scheduled, init container starts                         │
│  4. watcher-wait polls GetWatchState RPC                         │
│  5. Argusd creates watch SYNCHRONOUSLY (watches initialized)     │
│  6. GetWatchState returns watches_ready=true                     │
│  7. watcher-wait exits 0 → main containers start (PROTECTED)    │
│                                                                  │
│  Defense layers:                                                 │
│  ✓ Synchronous watch init (watches registered before response)  │
│  ✓ Readiness fields in proto (watches_ready, ready_at)          │
│  ✓ Webhook + init container (blocks pod until ready)            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Enabling Webhook Injection

1. **Label the namespace** to enable watcher injection:
   ```bash
   kubectl label namespace <namespace> argus.panoptes.io/watcher-injection=enabled
   ```

2. **Create an ArgusWatcher** that selects pods in that namespace:
   ```yaml
   apiVersion: argus.como-technologies.io/v1
   kind: ArgusWatcher
   metadata:
     name: my-watcher
     namespace: <namespace>
   spec:
     selector:
       matchLabels:
         app: my-app
     subjects:
       - paths: [/etc/passwd]
         events: [modify]
   ```

3. **Deploy pods** - the webhook will automatically inject the `wait-for-watcher` init container.

### Disabling Injection for Specific Pods

Add the annotation `argus.panoptes.io/inject: "false"` to bypass injection:

```yaml
metadata:
  annotations:
    argus.panoptes.io/inject: "false"
```

### Configuration

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `WATCHER_WAIT_IMAGE` | Image for the watcher-wait init container | `panoptes/watcher-wait:latest` |
| `ARGUSD_ADDRESS` | Address of the argusd gRPC service | `http://argusd.panoptes-system:50051` |
| `WATCHER_MAX_WAIT_SECS` | Maximum time to wait for watcher readiness | `30` |

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
kubectl patch deployment argus-operator-controller-manager -n panoptes-system --type='json' \
  -p='[{"op": "add", "path": "/spec/template/spec/containers/0/args/-", "value": "--enable-webhook=true"}]'
```

## Components

### Operator (This Repository)

The Argus Operator is a Kubernetes controller built with [Kubebuilder](https://kubebuilder.io).
It watches ArgusWatcher custom resources and coordinates with argusd daemons to set up
file watches.

**Kubebuilder-generated scaffolding:**
- `cmd/main.go` - Entry point with controller-runtime setup
- `internal/controller/suite_test.go` - Test suite setup
- `config/` - Kustomize manifests (CRDs, RBAC, deployment)
- `Makefile` - Standard kubebuilder targets
- `Dockerfile` - Multi-stage build

**Custom implementation:**
- `api/v1/arguswatcher_types.go` - v1 CRD types with full spec/status
- `api/v1alpha1/arguswatcher_types.go` - Legacy v1alpha1 types
- `api/v1alpha1/arguswatcher_conversion.go` - Hub-spoke conversion
- `internal/controller/arguswatcher_controller.go` - Reconciliation logic
- `internal/grpc/client.go` - gRPC client for argusd communication

### Daemon (argusd)

The argusd daemon runs as a DaemonSet on each node. It receives watch requests from the
operator via gRPC and sets up inotify watches on container filesystems.

See [daemons/argusd/README.md](../../daemons/argusd/README.md) for daemon documentation.

## CRD Reference

### ArgusWatcher v1

```yaml
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: example-watcher
  namespace: default
spec:
  # Pod selector (required)
  selector:
    matchLabels:
      app: my-app
    matchExpressions:
      - key: environment
        operator: In
        values: ["production", "staging"]

  # Watch subjects (required, max 20)
  subjects:
    - # Paths to watch (required, max 100 per subject)
      paths:
        - /etc/passwd
        - /etc/shadow
        - /app/config/

      # Events to monitor (required)
      # Options: access, attrib, closewrite, closenowrite, close,
      #          create, delete, deleteself, modify, moveself,
      #          movedfrom, movedto, move, open, all
      events:
        - modify
        - delete
        - create

      # Glob patterns to ignore
      ignore:
        - "*.tmp"
        - "*.swp"

      # Watch subdirectories recursively
      recursive: true

      # Maximum recursion depth (optional)
      maxDepth: 10

      # Only watch directory events
      onlyDir: false

      # Follow files when moved
      followMove: true

      # Custom metadata tags
      tags:
        severity: critical
        compliance: pci-dss

  # Container runtime: containerd, cri-o, or auto (default: auto)
  containerRuntime: auto

  # Custom log format template
  logFormat: "{{.Timestamp}} {{.Event}} {{.Path}}"

  # Temporarily disable monitoring
  paused: false

status:
  # Last observed spec generation
  observedGeneration: 1

  # Number of pods matching selector
  observablePods: 5

  # Number of pods with active watches
  watchedPods: 5

  # Total events detected
  eventsDetected: 1234

  # Per-pod status
  podStatuses:
    - podName: my-app-abc123
      nodeName: worker-1
      watchCount: 15
      lastEventTime: "2026-01-10T10:30:00Z"
      conditions:
        - type: Watching
          status: "True"
          reason: WatchesActive
          message: "15 watches active"

  # Standard Kubernetes conditions
  conditions:
    - type: Ready
      status: "True"
      reason: Reconciled
      message: "All pods watched successfully"
```

### v1alpha1 to v1 Migration

The v1 API includes these improvements over v1alpha1:

| Feature | v1alpha1 | v1 |
|---------|----------|-----|
| Pausing | Not supported | `spec.paused` |
| Per-pod status | Not supported | `status.podStatuses[]` |
| Event counters | Not supported | `status.eventsDetected` |
| Container runtime | `containerEngine` (docker/rkt) | `containerRuntime` (containerd/cri-o/auto) |
| Validation | Basic | MaxItems, MaxLength, enums |
| Conditions | Not supported | Standard Kubernetes conditions |

Conversion webhooks automatically handle v1alpha1 resources.

## Installation

### Prerequisites

- Kubernetes 1.28+
- containerd or CRI-O container runtime
- Go 1.22+ (for building)
- Docker 17.03+ (for building images)

### Deploy with kubectl

```bash
# Install CRDs
make install

# Deploy operator
make deploy IMG=<your-registry>/argus-operator:v2.0.0
```

### Deploy with Helm

```bash
helm install argus ./charts/argus-operator \
  --namespace argus-system \
  --create-namespace
```

### Build from Source

```bash
# Generate code and manifests
make generate manifests

# Build binary
make build

# Build and push Docker image
make docker-build docker-push IMG=<your-registry>/argus-operator:v2.0.0
```

## Configuration

### Operator Flags

| Flag | Environment Variable | Default | Description |
|------|---------------------|---------|-------------|
| `--metrics-bind-address` | `METRICS_BIND_ADDRESS` | `:8080` | Metrics endpoint |
| `--health-probe-bind-address` | `HEALTH_PROBE_BIND_ADDRESS` | `:8081` | Health probes endpoint |
| `--leader-elect` | `LEADER_ELECT` | `false` | Enable leader election |
| `--argusd-port` | `ARGUSD_PORT` | `50051` | gRPC port for argusd |

### RBAC

The operator requires these permissions:

```yaml
# ArgusWatcher resources
- apiGroups: ["argus.como-technologies.io"]
  resources: ["arguswatchers"]
  verbs: ["get", "list", "watch"]
- apiGroups: ["argus.como-technologies.io"]
  resources: ["arguswatchers/status"]
  verbs: ["get", "update", "patch"]
- apiGroups: ["argus.como-technologies.io"]
  resources: ["arguswatchers/finalizers"]
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
| `argus_reconcile_total` | Counter | Total reconciliations |
| `argus_reconcile_duration_seconds` | Histogram | Reconciliation duration |
| `argus_watched_pods` | Gauge | Pods with active watches |
| `argus_active_watches` | Gauge | Total inotify watches |
| `argus_events_total` | Counter | File events by type |
| `argus_grpc_requests_total` | Counter | gRPC requests to daemons |
| `argus_grpc_request_duration_seconds` | Histogram | gRPC request duration |

### Health Endpoints

- `/healthz` - Liveness probe
- `/readyz` - Readiness probe
- `/metrics` - Prometheus metrics

## Examples

### Monitor Critical System Files

```yaml
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: system-files
spec:
  selector:
    matchLabels:
      app.kubernetes.io/part-of: my-system
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/group
        - /etc/sudoers
        - /etc/sudoers.d/
      events: [modify, delete, attrib]
      recursive: true
      tags:
        severity: critical
```

### Monitor Application Configs

```yaml
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: app-configs
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - paths:
        - /app/config/
        - /app/secrets/
      events: [modify, create, delete]
      recursive: true
      ignore:
        - "*.bak"
        - "*.tmp"
      tags:
        team: platform
```

### Monitor with Custom Log Format

```yaml
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: audit-watcher
spec:
  selector:
    matchLabels:
      audit: enabled
  logFormat: |
    {"time":"{{.Timestamp}}","event":"{{.Event}}","path":"{{.Path}}","pod":"{{.Pod}}"}
  subjects:
    - paths: [/var/log/audit/]
      events: [all]
```

## Troubleshooting

### Watcher not detecting events

1. Check operator logs:
   ```bash
   kubectl logs -n argus-system deployment/argus-operator
   ```

2. Verify daemon is running:
   ```bash
   kubectl get pods -n argus-system -l app=argusd
   ```

3. Check daemon logs:
   ```bash
   kubectl logs -n argus-system -l app=argusd
   ```

4. Verify selector matches pods:
   ```bash
   kubectl get pods -l <your-selector>
   ```

### High inotify watch count

Linux has a limit on inotify watches. Check and increase if needed:

```bash
# Check current limit
cat /proc/sys/fs/inotify/max_user_watches

# Increase limit
echo 524288 | sudo tee /proc/sys/fs/inotify/max_user_watches
```

### Permission denied errors

Ensure argusd has required capabilities:
- `SYS_ADMIN` - Access container namespaces
- `SYS_PTRACE` - Access /proc for container PIDs
- `DAC_READ_SEARCH` - Traverse container filesystems

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
argus-operator/
├── api/
│   ├── v1/                    # Stable v1 API (hub)
│   │   ├── arguswatcher_types.go
│   │   ├── groupversion_info.go
│   │   └── zz_generated.deepcopy.go
│   └── v1alpha1/              # Legacy v1alpha1 API (spoke)
│       ├── arguswatcher_types.go
│       ├── arguswatcher_conversion.go
│       ├── groupversion_info.go
│       └── zz_generated.deepcopy.go
├── cmd/
│   └── main.go                # Entry point (kubebuilder generated)
├── config/
│   ├── crd/                   # CRD manifests
│   ├── rbac/                  # RBAC manifests
│   ├── manager/               # Operator deployment
│   ├── samples/               # Example CRs
│   └── webhook/               # Conversion webhook config
├── internal/
│   ├── controller/            # Reconciliation logic
│   │   ├── arguswatcher_controller.go
│   │   └── suite_test.go
│   └── grpc/                  # gRPC client for argusd
│       └── client.go
├── Dockerfile
├── Makefile
└── PROJECT                    # Kubebuilder project file
```

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](../../LICENSE) for details.

## Acknowledgments

Based on the original [Argus project](https://clustergarage.io/argus/) by ClusterGarage.
