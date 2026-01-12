# Changelog

All notable changes to the Argus/Janus security suite are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-01-10

### Major Version: v1alpha1 to v1 API Migration

This release represents a complete modernization of the ClusterGarage Argus (v0.4.0) and
Janus (v0.1.0) projects originally created circa 2018. The codebase has been updated for
modern Kubernetes (1.28+) patterns, security best practices, and observability standards.

### API Changes

#### ArgusWatcher CRD

| Feature | v1alpha1 (2018) | v1 (2026) |
|---------|-----------------|-----------|
| API Version | `argus.como-technologies.io/v1alpha1` | `argus.como-technologies.io/v1` |
| Pausing | Not supported | `spec.paused` field |
| Per-pod status | Not supported | `status.podStatuses[]` with conditions |
| Event counters | Not supported | `status.eventsDetected` |
| Array validation | None | MaxItems=100 paths, MaxItems=20 subjects |
| Path validation | None | MaxLength=1024 |
| Container runtime | `spec.containerEngine` (docker/rkt) | `spec.containerRuntime` (containerd/cri-o/auto) |
| Status conditions | Not supported | Standard Kubernetes conditions |
| Observed generation | Not supported | `status.observedGeneration` |
| Short names | None | `aw` |

#### JanusGuard CRD

| Feature | v1alpha1 (2018) | v1 (2026) |
|---------|-----------------|-----------|
| API Version | `janus.como-technologies.io/v1alpha1` | `janus.como-technologies.io/v1` |
| Pausing | Not supported | `spec.paused` field |
| Dry-run mode | Not supported | `spec.enforcing=false` |
| Per-pod status | Not supported | `status.podStatuses[]` with conditions |
| Event counters | Not supported | `status.totalDeniedEvents`, `status.totalAuditedEvents` |
| Array validation | None | MaxItems=100 paths, MaxItems=20 subjects |
| Path validation | None | MaxLength=1024 |
| Container runtime | `spec.containerEngine` (docker/rkt) | `spec.containerRuntime` (containerd/cri-o/auto) |
| Status conditions | Not supported | Standard Kubernetes conditions |
| Observed generation | Not supported | `status.observedGeneration` |
| Short names | None | `jg` |

### Added

#### Operators
- **Stable v1 APIs** for both ArgusWatcher and JanusGuard CRDs
- **Conversion webhooks** using hub-spoke pattern for v1alpha1 backward compatibility
- **Kubebuilder v4** scaffolding with controller-runtime v0.17+
- **Pausing support** (`spec.paused`) to temporarily disable monitoring without deletion
- **Dry-run mode** (`spec.enforcing`) for JanusGuard audit-only operation
- **Per-pod status tracking** (`status.podStatuses[]`) with individual pod conditions
- **Event counters** in status for operational visibility and alerting
- **OpenAPI validation** with MaxItems, MaxLength, and enum constraints
- **Prometheus metrics** for controller reconciliation and daemon communication
- **OpenTelemetry tracing** for distributed observability
- **Structured JSON logging** with configurable verbosity
- **Leader election** for HA deployments
- **Graceful shutdown** handling with finalizers

#### Daemons
- **Modern CMake build system** (3.20+) with C17 and C++20 standards
- **gRPC 1.60+** for high-performance daemon communication
- **spdlog** for structured JSON logging
- **Rust alternative implementations** using nix crate for direct kernel syscalls
- **containerd support** via `/run/containerd/containerd.sock`
- **CRI-O support** via `/var/run/crio/crio.sock`
- **Multi-stage Dockerfiles** with distroless runtime images
- **Automatic container runtime detection** (auto mode)
- **Health check gRPC service** for Kubernetes probes

#### Proto Definitions
- **Versioned proto packages** (`argus.v1`, `janus.v1`)
- **Streaming RPC** for real-time event delivery
- **Health service** following gRPC health checking protocol
- **Extensive field documentation** with comments

#### Infrastructure
- **Spectro Cloud pack structure** for argus-fim and janus-audit
- **Helm charts** with values.schema.json validation
- **ServiceMonitor** resources for Prometheus Operator
- **Grafana dashboards** for security event visualization
- **PrometheusRule** resources for alerting

### Changed

- **Kubernetes minimum version**: 1.9 raised to 1.28
- **Controller framework**: Migrated from raw client-go to controller-runtime (kubebuilder)
- **Container runtimes**: Removed Docker/rkt support, added containerd/CRI-O
- **Build system**: Migrated from Makefile to CMake 3.20+
- **Logging**: Migrated from custom logging to spdlog (C++) and tracing (Rust)
- **gRPC version**: Upgraded from 1.x to 1.60+
- **Protobuf**: Migrated from proto2 to proto3 syntax
- **Copyright**: Transferred from ClusterGarage to Como Technologies, LTD

### Removed

- **Docker runtime support** (deprecated in Kubernetes 1.20, removed in 1.24)
- **rkt runtime support** (project abandoned in 2019)
- **Legacy cgroup v1 paths** (cgroup v2 is now standard)
- **Custom controller implementation** (replaced with controller-runtime)
- **Makefile build system** (replaced with CMake)

### Security

- **Least-privilege RBAC** with minimal required permissions
- **Pod Security Standards** compliance (restricted profile for controllers)
- **Non-root container execution** for controllers
- **Distroless base images** for reduced attack surface
- **TLS support** for gRPC communication
- **Network policies** for pod isolation

### Migration Guide

#### Automatic Migration (Recommended)

With conversion webhooks enabled, existing v1alpha1 resources are automatically
converted when accessed through the v1 API. No manual migration is required.

1. Deploy the new operator with conversion webhook enabled
2. Existing ArgusWatcher/JanusGuard v1alpha1 resources continue to work
3. New resources should use the v1 API

#### Manual Migration

To manually migrate resources:

1. Export existing resources:
   ```bash
   kubectl get arguswatchers.argus.como-technologies.io -A -o yaml > watchers.yaml
   kubectl get janusguards.janus.como-technologies.io -A -o yaml > guards.yaml
   ```

2. Update apiVersion in manifests:
   ```yaml
   # Before
   apiVersion: argus.como-technologies.io/v1alpha1

   # After
   apiVersion: argus.como-technologies.io/v1
   ```

3. Update container runtime field:
   ```yaml
   # Before
   spec:
     containerEngine: docker

   # After
   spec:
     containerRuntime: containerd  # or cri-o, auto
   ```

4. Apply updated resources after deploying new operator

### Known Issues

- Rust daemon implementations are scaffolds and not production-ready
- Benchmark comparison between C and Rust implementations pending

---

## [0.4.0] - 2018-xx-xx (ClusterGarage Argus)

### Original Argus Release

The original ClusterGarage Argus project providing Kubernetes-native file integrity
monitoring using Linux inotify.

#### Features
- ArgusWatcher CRD (v1alpha1)
- argusd daemon with inotify monitoring
- Docker and rkt container runtime support
- Real-time file change notifications
- Recursive directory watching
- Custom log format templating

#### Components
- argus-controller (Go, client-go based)
- argusd (C with gRPC)
- argus-eye (React visualization UI)

---

## [0.1.0] - 2018-xx-xx (ClusterGarage Janus)

### Original Janus Release

The original ClusterGarage Janus project providing Kubernetes-native file access
auditing using Linux fanotify.

#### Features
- JanusGuard CRD (v1alpha1)
- janusd daemon with fanotify monitoring
- Docker and rkt container runtime support
- Allow/deny pattern-based access control
- Kernel audit log integration
- Real-time access event streaming

#### Components
- janus-controller (Go, client-go based)
- janusd (C with gRPC)

---

## Original Project Credits

The Argus and Janus projects were originally created by ClusterGarage:
- https://clustergarage.io/argus/
- https://clustergarage.io/janus/
- https://github.com/clustergarage/argus
- https://github.com/clustergarage/janus

This modernization effort preserves the core functionality while updating for
contemporary Kubernetes environments and best practices.
