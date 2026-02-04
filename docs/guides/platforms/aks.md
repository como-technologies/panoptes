# Panoptes on Azure Kubernetes Service (AKS)

## Supported Configurations

| Node Type | Argus (inotify) | Janus (fanotify) | Notes |
|-----------|----------------|-------------------|-------|
| Linux (Ubuntu) | Yes | Yes | Default, recommended |
| Linux (Azure Linux / Mariner) | Yes | Yes | Microsoft's CBL-Mariner based OS |
| Windows | No | No | Linux kernel interfaces required |
| Virtual nodes (ACI) | No | No | No host access |

## Prerequisites

- AKS cluster 1.28+ with Linux node pools
- `az` and `kubectl` CLI configured
- System node pool with at least Standard_B2s instances

## Deployment

### Helm Install

```bash
helm install panoptes ./packs/panoptes/charts/panoptes \
  --namespace panoptes-system --create-namespace \
  --set global.cluster.name="aks-prod" \
  --set global.cluster.environment="production" \
  --set global.cluster.region="eastus"
```

### Azure Workload Identity

For clusters using Azure Workload Identity:

```yaml
controller:
  serviceAccount:
    annotations:
      azure.workload.identity/client-id: CLIENT_ID
    labels:
      azure.workload.identity/use: "true"
```

## Windows Node Exclusion

If your cluster has mixed Linux and Windows node pools, Panoptes automatically schedules only on Linux nodes via the DaemonSet's default tolerations. However, explicitly adding a nodeSelector is recommended:

```yaml
daemon:
  nodeSelector:
    kubernetes.io/os: linux
controller:
  nodeSelector:
    kubernetes.io/os: linux
```

## Azure CNI Considerations

### Azure CNI (default)

Works without additional configuration. NetworkPolicy enforcement requires enabling Azure Network Policy or Calico:

```bash
# Check if network policy is enabled
az aks show -g RESOURCE_GROUP -n CLUSTER_NAME \
  --query "networkProfile.networkPolicy"
```

If network policy is not enabled, Panoptes's NetworkPolicy templates will have no effect (they won't break anything, but won't restrict traffic).

### Azure CNI Overlay

Compatible with Panoptes. No special configuration needed.

### Kubenet

Compatible, but NetworkPolicy requires Calico addon.

## Azure Policy Interaction

If you use Azure Policy for AKS, it may block Panoptes DaemonSets due to:

- `hostPID: true` requirement
- Required capabilities (`SYS_ADMIN`, `SYS_PTRACE`, `DAC_READ_SEARCH`)
- `hostPath` volume mounts

Create an exemption for the `panoptes-system` namespace:

```bash
# Label the namespace for policy exemption
kubectl label namespace panoptes-system \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/warn=privileged
```

Or create an Azure Policy exemption for the Panoptes namespace in the Azure Portal.

## Container Runtime

AKS uses containerd on all supported Kubernetes versions. Panoptes auto-detects this.

## inotify Tuning

Azure Linux nodes may need inotify limit increases for large workloads:

```bash
# Check current limits
kubectl debug node/NODE_NAME -it --image=busybox -- \
  cat /proc/sys/fs/inotify/max_user_watches

# Tune via DaemonSet init container or node pool configuration
# See docs/operations/kernel-tuning.md
```

For AKS node pool custom configuration, use the `--linux-os-config` flag:

```bash
az aks nodepool add \
  --resource-group RESOURCE_GROUP \
  --cluster-name CLUSTER_NAME \
  --name panoptespool \
  --linux-os-config linuxOsConfig.json
```

Where `linuxOsConfig.json` contains:
```json
{
  "sysctls": {
    "fsInotifyMaxUserWatches": 1048576,
    "fsInotifyMaxUserInstances": 8192
  }
}
```

## Monitoring Integration

### Azure Monitor / Container Insights

AKS Container Insights collects container logs automatically. Panoptes daemons log structured JSON to stdout, which is collected by the Azure Monitor agent.

### Azure Managed Prometheus + Grafana

If using Azure Managed Prometheus:

1. Enable Azure Monitor managed service for Prometheus on your cluster
2. Panoptes ServiceMonitor resources are automatically discovered
3. Import `deploy/monitoring/grafana-dashboard.json` into Azure Managed Grafana

## Virtual Node Limitations

AKS virtual nodes (Azure Container Instances) do not support Panoptes because:

- No DaemonSet scheduling on virtual nodes
- No host filesystem access
- No Linux capability additions

Ensure Panoptes pods schedule only on real VM-backed node pools.

## Troubleshooting

### Pods blocked by Azure Policy

Check for policy violations:

```bash
kubectl get events -n panoptes-system --field-selector reason=FailedCreate
```

### AKS node pool not ready

Verify the node pool is in a healthy state:

```bash
az aks nodepool show -g RESOURCE_GROUP -n NODEPOOL_NAME \
  --cluster-name CLUSTER_NAME --query provisioningState
```

### Upgrade considerations

When upgrading AKS, Panoptes DaemonSets will be evicted and rescheduled. The PDB ensures at least one controller replica remains available. Daemon pods will be recreated on each node after the upgrade completes.
