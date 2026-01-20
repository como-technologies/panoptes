# Multi-Cluster Monitoring with Panoptes

This guide explains how to deploy Panoptes across multiple Kubernetes clusters and aggregate monitoring data using Prometheus federation.

## Architecture Overview

In a multi-cluster deployment:
- Each cluster runs its own Panoptes suite (Argus, Janus, panoptes-eye)
- Events are labeled with `cluster_name` for identification
- Prometheus federation aggregates metrics across clusters
- Each cluster's panoptes-eye provides local management

```
┌─────────────────────────────────────────────────────────────────┐
│                    Central Observability                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │  Prometheus     │  │   Grafana       │  │  AlertManager   │ │
│  │  (Federation)   │  │  (Dashboards)   │  │  (Alerts)       │ │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘ │
└───────────┼───────────────────┼──────────────────────┼─────────┘
            │                   │                      │
     ┌──────┴────────┬─────────┴─────────┬────────────┴──────┐
     │               │                   │                   │
     ▼               ▼                   ▼                   ▼
┌─────────┐   ┌─────────┐         ┌─────────┐         ┌─────────┐
│ Cluster │   │ Cluster │         │ Cluster │         │ Cluster │
│ prod-1  │   │ prod-2  │         │ staging │         │  dev    │
└─────────┘   └─────────┘         └─────────┘         └─────────┘
```

## Configuration

### 1. Set Cluster Name

Each cluster needs a unique identifier. Configure this in your Helm values:

```yaml
# values.yaml
global:
  cluster:
    name: "prod-east-1"        # Unique cluster identifier
    environment: "production"   # Optional: environment label
    region: "us-east-1"        # Optional: region label
```

### Spectro Cloud Palette Auto-Injection

When deploying via Spectro Cloud Palette, use system macros for automatic cluster identification:

```yaml
# Per-cluster pack overrides in Palette
global:
  cluster:
    name: "{{ .spectro.system.cluster.name }}"
    # Or use manual naming: name: "prod-east-1"
```

Available Palette macros:
- `{{ .spectro.system.cluster.name }}` - Cluster name from Palette
- `{{ .spectro.system.cluster.uid }}` - Unique cluster UID
- `{{ .spectro.system.project.name }}` - Project name

### 2. Enable Metrics Labels

Events will automatically include `cluster_name` in all metrics and events. The daemons read this from the `PANOPTES_CLUSTER_NAME` environment variable.

## Prometheus Federation Setup

### Central Prometheus Configuration

On your central Prometheus instance, configure federation to scrape from each cluster:

```yaml
# prometheus.yml (central)
scrape_configs:
  # Federate from prod-east-1
  - job_name: 'federate-prod-east-1'
    honor_labels: true
    metrics_path: '/federate'
    params:
      'match[]':
        - '{job=~"panoptes.*"}'
        - '{__name__=~"argus_.*"}'
        - '{__name__=~"janus_.*"}'
    static_configs:
      - targets:
        - 'prometheus.prod-east-1.svc:9090'
    relabel_configs:
      - source_labels: [__address__]
        target_label: cluster
        replacement: 'prod-east-1'

  # Federate from prod-west-1
  - job_name: 'federate-prod-west-1'
    honor_labels: true
    metrics_path: '/federate'
    params:
      'match[]':
        - '{job=~"panoptes.*"}'
        - '{__name__=~"argus_.*"}'
        - '{__name__=~"janus_.*"}'
    static_configs:
      - targets:
        - 'prometheus.prod-west-1.svc:9090'
    relabel_configs:
      - source_labels: [__address__]
        target_label: cluster
        replacement: 'prod-west-1'
```

### Cross-Cluster Networking

For federation to work, the central Prometheus needs network access to each cluster's Prometheus. Options include:

1. **VPN/Private Network**: Clusters connected via VPN
2. **Ingress with Authentication**: Expose Prometheus via authenticated ingress
3. **Spectro Cloud Palette**: Use Palette's built-in cluster networking

### Thanos/Cortex Alternative

For larger deployments, consider using Thanos or Cortex for long-term storage and global querying:

```yaml
# thanos-sidecar on each cluster's Prometheus
spec:
  thanos:
    image: quay.io/thanos/thanos:v0.34.0
    objectStorageConfig:
      name: thanos-objstore-config
      key: thanos.yaml
    externalLabels:
      cluster: "prod-east-1"
```

## Grafana Dashboards

### Multi-Cluster Overview Dashboard

Create a dashboard variable for cluster selection:

```json
{
  "name": "cluster",
  "type": "query",
  "query": "label_values(argus_events_total, cluster)",
  "multi": true,
  "includeAll": true
}
```

### Example Queries

```promql
# Total events across all clusters
sum(rate(argus_events_total[5m])) by (cluster)

# Denied access attempts per cluster
sum(rate(janus_denied_total[5m])) by (cluster)

# Critical file modifications (all clusters)
sum(rate(argus_events_total{severity="critical"}[5m])) by (cluster, path)
```

## AlertManager Configuration

Route alerts based on cluster:

```yaml
# alertmanager.yml
route:
  receiver: 'default'
  routes:
    - match:
        cluster: 'prod-east-1'
      receiver: 'prod-east-team'
    - match:
        cluster: 'prod-west-1'
      receiver: 'prod-west-team'

receivers:
  - name: 'prod-east-team'
    slack_configs:
      - channel: '#prod-east-alerts'
  - name: 'prod-west-team'
    slack_configs:
      - channel: '#prod-west-alerts'
```

## Event Streaming

Events streamed via gRPC from the daemons include `cluster_name`:

```protobuf
message FileEvent {
  // ... other fields ...
  string cluster_name = 14;  // e.g., "prod-east-1"
}

message AccessEvent {
  // ... other fields ...
  string cluster_name = 14;  // e.g., "prod-east-1"
}
```

Use this in your event consumers to filter or route events:

```python
# Example Python consumer
for event in stream_events():
    if event.cluster_name == "prod-east-1":
        # Handle prod-east events
        pass
```

## Best Practices

1. **Unique Cluster Names**: Use a consistent naming scheme (e.g., `{env}-{region}-{index}`)
2. **Label Consistency**: Keep environment/region labels consistent across clusters
3. **Retention Policies**: Configure appropriate retention at central and edge
4. **Network Security**: Use TLS and authentication for cross-cluster communication
5. **Failover**: Design for graceful degradation if central observability is unavailable

## Troubleshooting

### Cluster Name Not Appearing in Events

1. Check the daemon logs for `cluster=` in the startup message
2. Verify `PANOPTES_CLUSTER_NAME` env var is set:
   ```bash
   kubectl exec -n panoptes-system ds/argusd -- printenv | grep CLUSTER
   ```
3. Ensure Helm values include `global.cluster.name`

### Federation Not Working

1. Test connectivity from central Prometheus:
   ```bash
   curl -v http://prometheus.remote-cluster:9090/federate?match[]={__name__=~"argus.*"}
   ```
2. Check firewall rules and network policies
3. Verify ServiceMonitor is scraping metrics

## Related Documentation

- [Quick Start Security](./quick-start-security.md)
- [Compliance Templates](../compliance-templates/README.md)
- [Spectro Cloud Quick Start](../SPECTRO_QUICK_START.md)
