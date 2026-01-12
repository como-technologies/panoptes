# Panoptes Suite

**Kubernetes-native file integrity monitoring and access auditing**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-1.28%2B-blue.svg)](https://kubernetes.io)

The Panoptes Suite provides real-time file integrity monitoring (FIM) and file access
auditing for Kubernetes workloads. Built on Linux kernel interfaces (inotify and fanotify),
it enables security teams to detect unauthorized file modifications and control file access
at the container level.

## Components

| Component | Purpose | Kernel Interface |
|-----------|---------|------------------|
| **Argus** | File Integrity Monitoring | inotify |
| **Janus** | File Access Auditing | fanotify |

### Argus - File Integrity Monitoring

Argus monitors file system changes in real-time using Linux inotify. It detects:
- File creation, modification, and deletion
- Directory changes
- Attribute modifications
- File moves and renames

Use cases: Compliance monitoring (PCI-DSS 10.5.5), configuration drift detection,
security incident detection, audit logging.

### Janus - File Access Auditing

Janus controls and audits file access using Linux fanotify. It provides:
- Allow/deny policies for file access
- Real-time access event streaming
- Kernel audit log integration
- Permission-based enforcement

Use cases: Runtime protection, access control enforcement, security auditing,
compliance evidence collection (SOC2, HIPAA).

## Philosophy

Panoptes follows the Unix philosophy: **one tool, one job, done well.**

While enterprise security platforms have become bloated with AI/ML detection,
thousands of pre-built policies, and auto-remediation engines, we take the
opposite approach:

**Tried. Tested. Obvious. Explainable.**

### Core Principles

1. **Kernel-level detection**: Linux inotify and fanotify—the same mechanisms
   the kernel uses. No heuristics. No guessing.

2. **Kubernetes-native**: CRDs, operators, and label selectors. Not YAML soup
   or agent sprawl.

3. **Transparent operation**: Clear logs, obvious behavior. You can explain
   any alert to an auditor in one sentence.

4. **Expert-focused**: Built for security professionals who understand what
   they're monitoring.

5. **Composable**: Works with your existing observability stack—Prometheus,
   Grafana, Loki, your SIEM. We don't try to replace them.

### What We Don't Build

We explicitly avoid:
- AI/ML anomaly detection (black box, can't explain to auditors)
- Auto-remediation (dangerous, should involve humans)
- Complex policy engines (thousand-rule libraries no one audits)
- Built-in SIEM (compose with existing SIEMs instead)
- Risk scores (made-up numbers that aren't auditable)

See [docs/FUTURE_STATE.md](docs/FUTURE_STATE.md) for our complete philosophy
and roadmap.

## Architecture

```
                                    ┌─────────────────────────────────────────┐
                                    │           Kubernetes Cluster            │
                                    └─────────────────────────────────────────┘
                                                        │
                    ┌───────────────────────────────────┴───────────────────────────────────┐
                    │                                                                       │
            ┌───────▼───────┐                                                       ┌───────▼───────┐
            │    Argus      │                                                       │    Janus      │
            │   Operator    │                                                       │   Operator    │
            │  (Deployment) │                                                       │  (Deployment) │
            └───────┬───────┘                                                       └───────┬───────┘
                    │ watches ArgusWatcher CRs                                              │ watches JanusGuard CRs
                    │ manages DaemonSet                                                     │ manages DaemonSet
                    │                                                                       │
    ┌───────────────┴───────────────┐                               ┌───────────────────────┴───────────────┐
    │                               │                               │                                       │
┌───▼───┐   ┌───────┐   ┌───────┐   │                           ┌───▼───┐   ┌───────┐   ┌───────┐           │
│argusd │   │argusd │   │argusd │   │                           │janusd │   │janusd │   │janusd │           │
│Node 1 │   │Node 2 │   │Node N │   │                           │Node 1 │   │Node 2 │   │Node N │           │
└───┬───┘   └───┬───┘   └───┬───┘   │                           └───┬───┘   └───┬───┘   └───┬───┘           │
    │           │           │       │                               │           │           │               │
    │      inotify watches  │       │                               │      fanotify marks   │               │
    │           │           │       │                               │           │           │               │
┌───▼───────────▼───────────▼───────▼───┐                       ┌───▼───────────▼───────────▼───────────────▼───┐
│              Linux Kernel             │                       │                 Linux Kernel                  │
│         (inotify subsystem)           │                       │            (fanotify subsystem)              │
└───────────────────────────────────────┘                       └───────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- Kubernetes 1.28+
- containerd or CRI-O container runtime
- Helm 3.x (for Helm installation)
- Nodes with Linux kernel 5.x+ (for full fanotify support)

### Installation

#### Option 1: Helm

```bash
# Add the Helm repository
helm repo add panoptes https://charts.como.tech/security
helm repo update

# Install Argus (File Integrity Monitoring)
helm install argus panoptes/argus-fim \
  --namespace argus-system \
  --create-namespace

# Install Janus (File Access Auditing)
helm install janus panoptes/janus-audit \
  --namespace janus-system \
  --create-namespace
```

#### Option 2: kubectl

```bash
# Install Argus
kubectl apply -f https://raw.githubusercontent.com/como-technologies/panoptes/main/operators/argus-operator/config/deploy/install.yaml

# Install Janus
kubectl apply -f https://raw.githubusercontent.com/como-technologies/panoptes/main/operators/janus-operator/config/deploy/install.yaml
```

#### Option 3: Spectro Cloud Palette

Add the `argus-fim` and/or `janus-audit` packs to your cluster profile.

---

### Create Your First Watcher

```yaml
# argus-watcher.yaml
apiVersion: argus.como-technologies.io/v1
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
        - attrib
      recursive: false
      tags:
        severity: critical
        compliance: pci-dss
```

```bash
kubectl apply -f argus-watcher.yaml
```

### Create Your First Guard

```yaml
# janus-guard.yaml
apiVersion: janus.como-technologies.io/v1
kind: JanusGuard
metadata:
  name: sensitive-data
  namespace: default
spec:
  selector:
    matchLabels:
      app: my-app
  enforcing: true
  subjects:
    - allow:
        - /app/**
        - /tmp/**
      deny:
        - /etc/shadow
        - /root/**
      events:
        - open
        - access
      audit: true
      tags:
        policy: restrict-sensitive
```

```bash
kubectl apply -f janus-guard.yaml
```

## Configuration

### ArgusWatcher Spec

| Field | Type | Description |
|-------|------|-------------|
| `selector` | LabelSelector | Pods to monitor (required) |
| `subjects` | []Subject | Monitoring rules (required) |
| `subjects[].paths` | []string | Paths to watch (max 100) |
| `subjects[].events` | []string | Events to monitor: access, attrib, closewrite, closenowrite, close, create, delete, deleteself, modify, moveself, movedfrom, movedto, move, open, all |
| `subjects[].recursive` | bool | Watch subdirectories recursively |
| `subjects[].maxDepth` | int | Maximum recursion depth |
| `subjects[].ignore` | []string | Glob patterns to ignore |
| `subjects[].onlyDir` | bool | Only watch directories |
| `subjects[].followMove` | bool | Follow moved files |
| `subjects[].tags` | map | Custom metadata tags |
| `containerRuntime` | string | containerd, cri-o, or auto (default: auto) |
| `logFormat` | string | Custom log format template |
| `paused` | bool | Temporarily disable monitoring |

### JanusGuard Spec

| Field | Type | Description |
|-------|------|-------------|
| `selector` | LabelSelector | Pods to guard (required) |
| `subjects` | []Subject | Access rules (required) |
| `subjects[].allow` | []string | Allowed path patterns (max 100) |
| `subjects[].deny` | []string | Denied path patterns (max 100) |
| `subjects[].events` | []string | Events to guard: access, open, all |
| `subjects[].onlyDir` | bool | Only guard directories |
| `subjects[].autoAllowOwner` | bool | Allow access by file owner |
| `subjects[].audit` | bool | Log to kernel audit |
| `subjects[].tags` | map | Custom metadata tags |
| `containerRuntime` | string | containerd, cri-o, or auto (default: auto) |
| `logFormat` | string | Custom log format template |
| `paused` | bool | Temporarily disable guarding |
| `enforcing` | bool | Enforce deny rules (false = audit only) |

## Monitoring

### Prometheus Metrics

Both operators expose Prometheus metrics on port 8080:

**Argus Metrics:**
- `argus_events_total` - Total file events by type, watcher, namespace
- `argus_active_watches` - Current active inotify watches
- `argus_watched_paths_total` - Total paths being monitored
- `argus_reconcile_duration_seconds` - Controller reconciliation time

**Janus Metrics:**
- `janus_access_events_total` - Total access events by type, response
- `janus_denied_access_total` - Total denied access attempts
- `janus_active_guards` - Current active fanotify guards
- `janus_reconcile_duration_seconds` - Controller reconciliation time

### Grafana Dashboards

Import the provided dashboards from `charts/dashboards/`:
- `argus-overview.json` - File integrity monitoring overview
- `janus-overview.json` - Access auditing overview
- `security-compliance.json` - Compliance scorecard

### Alerting

Example PrometheusRule for critical file modifications:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: argus-alerts
spec:
  groups:
    - name: argus.critical
      rules:
        - alert: CriticalFileModified
          expr: |
            increase(argus_events_total{
              event_type=~"modify|delete",
              tags=~".*severity:critical.*"
            }[5m]) > 0
          labels:
            severity: critical
          annotations:
            summary: "Critical file modified in {{ $labels.namespace }}"
            description: "File {{ $labels.path }} was modified"
```

## Compliance

Panoptes addresses requirements across major compliance frameworks. See
[docs/FUTURE_STATE.md](docs/FUTURE_STATE.md) for the complete compliance control
mapping and gap analysis.

### PCI-DSS 4.0

| Requirement | Description | Status |
|-------------|-------------|--------|
| 10.3.4 | FIM on audit logs to detect unauthorized changes | Implemented |
| 11.5.2 | Alert on critical file modifications | Implemented |
| 11.5.2 | Weekly baseline comparison | Planned |
| 10.7 | 13-month log retention | Planned |

**Currently supported:** Real-time detection of file changes to `/var/log/`,
`/etc/`, and application paths with alert generation via Prometheus metrics.

**Planned:** Process attribution, content hashing for baseline verification,
compliance-ready reporting.

### SOC2 Trust Criteria

| Criterion | Description | Status |
|-----------|-------------|--------|
| CC6.1 | Logical access security over protected assets | Implemented |
| CC7.1 | Configuration change monitoring | Implemented |
| CC7.2 | Anomaly detection in system components | Implemented |

**Currently supported:** Access control enforcement via Janus deny rules,
audit logging of all access decisions, kernel audit integration.

**Planned:** Baseline snapshots, change context classification (authorized vs.
unauthorized).

### HIPAA Security Rule

| Section | Description | Status |
|---------|-------------|--------|
| 164.312(a) | Access controls for ePHI | Implemented |
| 164.312(b) | Audit controls—log and examine activity | Implemented |
| 164.312(c) | Integrity controls—prevent unauthorized alteration | Implemented |
| 164.530(j) | 6-year retention | Planned |

### NIST 800-53

| Control | Description | Status |
|---------|-------------|--------|
| SI-7 | Software/firmware/info integrity | Implemented |
| SI-7(1) | Cryptographic hash verification | Planned |
| SI-7(2) | Baseline comparison | Planned |
| AU-2/AU-3 | Audit event logging | Implemented |

### GDPR

| Article | Description | Status |
|---------|-------------|--------|
| Art. 32 | Security of processing, integrity measures | Implemented |
| Art. 30 | Records of processing activities | Planned |

## Directory Structure

```
panoptes/
├── operators/
│   ├── argus-operator/     # Argus Kubernetes operator
│   └── janus-operator/     # Janus Kubernetes operator
├── daemons/
│   ├── argusd/             # Argus node daemon
│   │   ├── c/              # C implementation (primary)
│   │   └── rust/           # Rust implementation (alternative)
│   └── janusd/             # Janus node daemon
│       ├── c/              # C implementation (primary)
│       └── rust/           # Rust implementation (alternative)
├── proto/
│   ├── argus/v1/           # Argus gRPC definitions
│   └── janus/v1/           # Janus gRPC definitions
├── packs/
│   ├── argus-fim/          # Spectro Cloud pack
│   └── janus-audit/        # Spectro Cloud pack
├── charts/                 # Helm charts
├── benchmarks/             # Performance benchmarks
└── docs/                   # Documentation
```

## Building from Source

### Operators

```bash
cd operators/argus-operator
make generate manifests
make docker-build IMG=your-registry/argus-operator:tag
make docker-push IMG=your-registry/argus-operator:tag
```

### Daemons (C-based)

```bash
cd daemons/argusd/c
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

### Daemons (Rust-based)

```bash
cd daemons/argusd/rust
cargo build --release
```

## Upgrading

### From v1alpha1 to v1

The v1 API includes conversion webhooks for automatic migration. Existing
v1alpha1 resources continue to work. See [CHANGELOG.md](CHANGELOG.md) for
detailed migration information.

Key changes:
- `spec.containerEngine` renamed to `spec.containerRuntime`
- Docker and rkt runtimes removed (use containerd or cri-o)
- New `spec.paused` field for temporary disable
- New `spec.enforcing` field for Janus dry-run mode
- Enhanced status with per-pod conditions

## Troubleshooting

### Daemon not receiving events

1. Check daemon logs: `kubectl logs -n argus-system -l app=argusd`
2. Verify container runtime socket exists
3. Ensure daemon has required capabilities (SYS_ADMIN, SYS_PTRACE, DAC_READ_SEARCH)

### High CPU usage on daemon

1. Reduce number of watched paths
2. Use `ignore` patterns to exclude noisy directories
3. Consider using `onlyDir: true` for directory-level monitoring

### Events not appearing in logs

1. Check operator logs: `kubectl logs -n argus-system -l app=argus-operator`
2. Verify CRD selector matches target pods
3. Check pod annotations for watcher/guard attachment

## Contributing

Contributions are welcome! Please read our contributing guidelines and submit
pull requests to the main repository.

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Acknowledgments

This project is a modernization of the original Argus and Janus projects created
by ClusterGarage:
- [Argus Documentation](https://clustergarage.io/argus/)
- [Janus Documentation](https://clustergarage.io/janus/)
