# Panoptes Deployment

This directory contains manifests for deploying the complete Panoptes stack.

## Quick Install

```bash
# Install everything to panoptes-system namespace
kubectl apply -k .
```

This installs:
- **Argus operator** - Manages ArgusWatcher CRDs (in argus-operator-system)
- **Janus operator** - Manages JanusGuard CRDs (in janus-operator-system)
- **argusd DaemonSet** - File integrity monitoring daemon (in panoptes-system)
- **janusd DaemonSet** - File access auditing daemon (in panoptes-system)
- **Panoptes Eye** - Web dashboard (in panoptes-system)

## Files

| File | Description |
|------|-------------|
| `kustomization.yaml` | Kustomize config for unified install |
| `namespace.yaml` | panoptes-system namespace |
| `argusd-daemonset.yaml` | Argus FIM daemon DaemonSet |
| `janusd-daemonset.yaml` | Janus audit daemon DaemonSet |
| `panoptes-eye.yaml` | Dashboard deployment + RBAC |

## Customization

### Change namespace

Edit `kustomization.yaml`:

```yaml
namespace: my-security-namespace
```

### Use custom images

Edit `kustomization.yaml`:

```yaml
images:
  - name: ghcr.io/como-technologies/argusd
    newName: my-registry/argusd
    newTag: v2.0.0
```

### Disable dashboard

Comment out the panoptes-eye resource in `kustomization.yaml`:

```yaml
resources:
  # ...
  # - panoptes-eye.yaml  # Disabled
```

### Multi-cluster deployment

Set cluster name in daemon configs:

```yaml
env:
  - name: PANOPTES_CLUSTER_NAME
    value: "prod-east-1"
```

## Verify Installation

```bash
# Check operators are running
kubectl get pods -n argus-operator-system
kubectl get pods -n janus-operator-system

# Check daemons and dashboard
kubectl get pods -n panoptes-system

# Verify CRDs are installed
kubectl get crd arguswatchers.argus.como-technologies.io
kubectl get crd janusguards.janus.como-technologies.io

# Access dashboard
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
# Open http://localhost:3000
```

## Uninstall

```bash
kubectl delete -k .
```
