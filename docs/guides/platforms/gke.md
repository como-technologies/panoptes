# Panoptes on Google Kubernetes Engine (GKE)

## Supported Configurations

| Node Type | Argus (inotify) | Janus (fanotify) | Notes |
|-----------|----------------|-------------------|-------|
| Standard (COS) | Yes | Yes | Default node image; requires DaemonSet tolerations |
| Standard (Ubuntu) | Yes | Yes | Full Linux kernel access |
| Autopilot | No | No | Autopilot restricts hostPID, hostPath, and privileged capabilities |
| ARM (T2A) | Yes | Yes | ARM64 images required |

## Prerequisites

- GKE cluster 1.28+ (Standard mode, not Autopilot)
- `gcloud` CLI configured
- Node pools with Container-Optimized OS (COS) or Ubuntu node images

## Deployment

### Helm Install

```bash
helm install panoptes ./packs/panoptes/charts/panoptes \
  --namespace panoptes-system --create-namespace \
  --set global.cluster.name="gke-prod" \
  --set global.cluster.environment="production" \
  --set global.cluster.region="us-central1"
```

### GKE Workload Identity

If your cluster uses Workload Identity, annotate the ServiceAccount:

```yaml
controller:
  serviceAccount:
    annotations:
      iam.gke.io/gcp-service-account: panoptes-sa@my-project.iam.gserviceaccount.com
```

## Container-Optimized OS (COS) Notes

COS is the default node image on GKE. Key considerations:

### Read-Only Filesystem

COS mounts most of the host filesystem as read-only. Panoptes mounts `/host` read-only by default, which is compatible.

### Kernel Module Restrictions

COS does not allow loading kernel modules. All required interfaces (inotify, fanotify) are built into the kernel, so this is not an issue.

### inotify Limits

Default COS inotify limits may be lower than needed for large workloads. Check and tune:

```bash
# Check current limits on a node (exec into a debug pod)
cat /proc/sys/fs/inotify/max_user_watches
cat /proc/sys/fs/inotify/max_user_instances

# On GKE, tune via DaemonSet init container or node pool metadata:
# See docs/operations/kernel-tuning.md for sysctl configuration
```

### Container Runtime

GKE uses containerd. Panoptes auto-detects this. The containerd socket is at the standard path `/run/containerd/containerd.sock`.

## Autopilot Limitations

GKE Autopilot does not support Panoptes because it restricts:

- `hostPID: true` (required for container PID resolution)
- `hostPath` volume mounts (required for host filesystem access)
- `SYS_ADMIN` capability (required for inotify/fanotify)

Use GKE Standard mode for Panoptes deployments.

## Network Policy

If you use GKE network policy enforcement (Calico or Dataplane V2), Panoptes includes built-in NetworkPolicy templates:

```yaml
networkPolicy:
  enabled: true  # Default: true
```

Ensure the GKE network policy controller is enabled:

```bash
gcloud container clusters describe CLUSTER_NAME \
  --format="value(networkPolicy.enabled)"
```

## Monitoring Integration

GKE integrates with Cloud Monitoring. To export Panoptes metrics:

1. Enable the ServiceMonitor (default):
   ```yaml
   observability:
     prometheus:
       serviceMonitor:
         enabled: true
   ```

2. If using Google Cloud Managed Prometheus, the ServiceMonitor is automatically scraped.

3. Import the Panoptes Grafana dashboard (`deploy/monitoring/grafana-dashboard.json`) into Cloud Monitoring Dashboards or a self-hosted Grafana instance.

## Troubleshooting

### DaemonSet pods not scheduling

Verify node pool taints and add corresponding tolerations:

```bash
kubectl describe nodes | grep Taints
```

### Permission denied errors

Verify the PodSecurityStandard allows privileged workloads in the panoptes-system namespace:

```bash
kubectl label namespace panoptes-system \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/warn=privileged
```
