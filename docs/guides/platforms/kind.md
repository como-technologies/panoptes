# Panoptes on Kind (Kubernetes in Docker)

Kind is the recommended platform for development, testing, and demos.

## Requirements

- Docker Desktop or Docker Engine
- Kind v0.20+
- Linux host or WSL2 (macOS Docker Desktop does not support the required kernel interfaces)

## Quick Setup

```bash
# Full automated setup
./hack/local-deploy.sh all

# WSL2 users
./hack/local-wsl-deploy.sh all
```

## Known Limitations

### inotify/fanotify Support

Kind nodes run as Docker containers sharing the host kernel. This means:

- **inotify**: Works correctly on Linux hosts. Does not work on macOS Docker Desktop (kernel is Linux VM, but inotify events may not propagate correctly through overlayfs layers).
- **fanotify**: Requires `CAP_SYS_ADMIN` on the host. Works on native Linux. May not work in all Docker Desktop configurations.

### WSL2 Specifics

WSL2 uses the Microsoft Linux kernel, which supports inotify and fanotify. However:

- Use `local-wsl-deploy.sh` instead of `local-deploy.sh` (pinned K8s 1.30.0, simplified networking)
- systemd cgroup driver must be enabled (the deploy script handles this)
- DNS resolution may require custom CoreDNS config if using a corporate VPN

### Resource Limits

Kind is single-node by default. The full Panoptes stack needs approximately:

| Component | CPU | Memory |
|-----------|-----|--------|
| Argus controller | 100m | 128Mi |
| Argus daemon | 50m | 64Mi |
| Janus controller | 100m | 128Mi |
| Janus daemon | 50m | 64Mi |
| Panoptes Eye | 100m | 128Mi |
| **Total** | **400m** | **512Mi** |

Ensure your Docker daemon has at least 2GB memory allocated.

### Container Runtime

Kind uses containerd by default. Panoptes auto-detects this. No additional configuration needed.

### Networking

Kind creates a Docker bridge network. If you need to access Panoptes Eye from outside:

```bash
# Port-forward is the simplest approach
kubectl port-forward svc/panoptes-eye -n panoptes-system 3000:80

# Or use Kind's extraPortMappings in the cluster config
```

## Troubleshooting

### Pods stuck in `Pending`

Check if the node has enough resources:

```bash
kubectl describe node kind-control-plane | grep -A5 "Allocated resources"
```

### Daemons crash with permission errors

Verify the container runtime supports the required capabilities:

```bash
kubectl logs -n panoptes-system -l app.kubernetes.io/component=daemon
```

On macOS, this is expected. Use a Linux host or WSL2.

### Events not appearing

Verify the watcher is targeting pods correctly:

```bash
kubectl get aw -A -o wide
kubectl describe aw <name>
```

Check that target pods have the correct labels.
