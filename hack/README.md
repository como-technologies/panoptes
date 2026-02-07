# Panoptes Deployment Scripts

This directory contains deployment scripts and configurations for running Panoptes in different environments.

---

## Quick Reference

| Script | Environment | Purpose |
|--------|-------------|---------|
| `local-deploy.sh` | Native Linux | Local kind cluster testing |
| `local-wsl-deploy.sh` | WSL2 | Local kind cluster on Windows/WSL2 |
| `spectro-deploy.sh` | Spectro Cloud | Deploy via Palette to managed clusters |

---

## Configuration Files

| File | Purpose |
|------|---------|
| `kind-config.yaml` | kind cluster config for native Linux (full features) |
| `kind-config-wsl.yaml` | kind cluster config for WSL2 (compatibility mode) |
| `panoptes-eye-local.yaml` | Kubernetes manifests for local dashboard deployment |

---

## Local Development (Native Linux)

For native Linux environments with full feature support.

### Prerequisites

```bash
# Docker 20.10+
docker --version

# kind 0.20+
kind --version

# kubectl 1.28+
kubectl version --client
```

### Quick Start

```bash
# Full deployment (cluster + build + deploy + test)
./hack/local-deploy.sh all

# Or step by step
./hack/local-deploy.sh cluster   # Create kind cluster
./hack/local-deploy.sh build     # Build container images
./hack/local-deploy.sh load      # Load images into kind
./hack/local-deploy.sh deploy    # Deploy Panoptes stack
./hack/local-deploy.sh test      # Create test resources
./hack/local-deploy.sh forward   # Start port-forwards
```

### Access

- **Panoptes Eye Dashboard**: http://localhost:3000
- **Argus Metrics**: http://localhost:8080/metrics
- **Janus Metrics**: http://localhost:8081/metrics

### Cleanup

```bash
./hack/local-deploy.sh clean
```

---

## Local Development (WSL2)

For Windows Subsystem for Linux 2 environments.

### Key Differences from Native Linux

| Feature | Native Linux | WSL2 |
|---------|--------------|------|
| Kubernetes version | Latest | Pinned to 1.30.0 |
| /proc mount | Enabled | Disabled (causes issues) |
| kubeadm patches | Full | Simplified |
| Registry patches | Enabled | Disabled |
| Timeouts | Standard | Extended |

### Prerequisites

```bash
# Docker Desktop with WSL2 integration
# - Enable "Use the WSL 2 based engine" in Docker Desktop settings
# - Enable WSL2 integration for your distro

# Verify Docker is accessible
docker info

# kind and kubectl (same as native Linux)
kind --version
kubectl version --client
```

### Quick Start

```bash
# Full deployment
./hack/local-wsl-deploy.sh all

# Or step by step (same commands as local-deploy.sh)
./hack/local-wsl-deploy.sh cluster
./hack/local-wsl-deploy.sh build
./hack/local-wsl-deploy.sh deploy
```

### Access

Access URLs are the same as native Linux. Open in your Windows browser:
- **Panoptes Eye Dashboard**: http://localhost:3000

### Troubleshooting WSL2

**Cluster creation times out:**
```bash
# WSL2 can be slower. Increase Docker Desktop resources:
# Settings → Resources → Memory: 4GB+, CPUs: 2+
```

**Docker not running:**
```bash
# Ensure Docker Desktop is started
# Check WSL2 integration: Settings → Resources → WSL Integration
```

**Cgroups error (unable to start container process):**
```bash
# Error: "unable to start container process: error adding pid X to cgroups"

# This is a known KIND/WSL2 issue due to cgroups configuration.
# See: https://kind.sigs.k8s.io/docs/user/known-issues/#failure-to-create-cluster-on-wsl2

# Quick check - if this file exists, cgroups v2 is enabled:
ls /sys/fs/cgroup/cgroup.controllers

# Fix by configuring cgroups v2 for WSL2:
# Follow the guide at: https://github.com/spurin/wsl-cgroupsv2
```

**Port conflicts:**
```bash
# Check if ports are in use
netstat.exe -an | findstr "3000 8080 8081"
```

---

## Spectro Cloud Palette Deployment

For deploying to managed Kubernetes clusters via Spectro Cloud.

### Prerequisites

```bash
# Option A: Palette CLI
# Install: https://docs.spectrocloud.com/palette-cli
palette version

# Option B: API Key
export PALETTE_API_KEY="your-api-key-here"
```

### Quick Start

```bash
# Login to Palette
./hack/spectro-deploy.sh login

# Push packs to registry
./hack/spectro-deploy.sh pack-push

# Create cluster profile
./hack/spectro-deploy.sh profile-create my-security-profile default

# Deploy to a cluster
./hack/spectro-deploy.sh deploy my-cluster my-security-profile

# Check status
./hack/spectro-deploy.sh status my-cluster
```

### Available Presets

| Preset | Description | Components |
|--------|-------------|------------|
| `default` | Standard deployment | Argus + Janus + Dashboard |
| `compliance` | PCI-DSS/SOC2 optimized | Extended retention, critical path monitoring |
| `minimal` | Lightweight | Argus only, no dashboard |

```bash
# Create profile with compliance preset
./hack/spectro-deploy.sh profile-create prod-security compliance
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PALETTE_API_KEY` | - | API key (alternative to CLI login) |
| `PALETTE_ENDPOINT` | api.spectrocloud.com | Palette API endpoint |
| `PALETTE_PROJECT` | Default | Project name |
| `PACK_VERSION` | 2.0.0 | Pack version to deploy |
| `REGISTRY_NAME` | spectro-packs | Pack registry name |

### Cleanup

```bash
# Remove Panoptes from cluster
./hack/spectro-deploy.sh clean my-cluster my-security-profile
```

---

## Environment Variables (All Scripts)

| Variable | Default | Description |
|----------|---------|-------------|
| `CLUSTER_NAME` | panoptes-dev | kind cluster name |
| `NAMESPACE` | panoptes-system | Kubernetes namespace |
| `IMAGE_TAG` | dev | Container image tag |

```bash
# Example: Custom cluster name and tag
CLUSTER_NAME=my-cluster IMAGE_TAG=v2.0.0 ./hack/local-deploy.sh all
```

---

## Script Commands Reference

### local-deploy.sh / local-wsl-deploy.sh

| Command | Description |
|---------|-------------|
| `cluster` | Create kind cluster only |
| `build` | Build container images |
| `load` | Load images into kind cluster |
| `deploy` | Deploy Panoptes stack |
| `test` | Create test resources and run tests |
| `forward` | Start port-forwards |
| `clean` | Delete cluster and resources |
| `all` | Run full setup |

### spectro-deploy.sh

| Command | Description |
|---------|-------------|
| `login` | Login to Spectro Cloud |
| `pack-push` | Push packs to registry |
| `profile-create [name] [preset]` | Create cluster profile |
| `deploy <cluster> [profile]` | Deploy to managed cluster |
| `status <cluster>` | Check deployment status |
| `clean <cluster> [profile]` | Remove from cluster |
| `list` | List available packs |

---

## Directory Structure

```
hack/
├── README.md                  # This file
├── kind-config.yaml           # kind config (native Linux)
├── kind-config-wsl.yaml       # kind config (WSL2)
├── local-deploy.sh            # Local deployment (native Linux)
├── local-wsl-deploy.sh        # Local deployment (WSL2)
├── spectro-deploy.sh          # Spectro Cloud deployment
├── panoptes-eye-local.yaml    # Dashboard K8s manifests
├── argusd-daemonset.yaml      # Argusd DaemonSet for local testing
└── janusd-daemonset.yaml      # Janusd DaemonSet for local testing
```

---

## Related Documentation

- [Quick Start - Local Testing](../docs/QUICK_START.md)
- [Quick Start - Spectro Cloud](../docs/SPECTRO_QUICK_START.md)
- [Future State & Roadmap](../docs/FUTURE_STATE.md)
- [Argus FIM Pack](../packs/panoptes-argus/README.md)
- [Janus Audit Pack](../packs/panoptes-janus/README.md)

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
