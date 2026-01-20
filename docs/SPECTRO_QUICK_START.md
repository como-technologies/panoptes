# Panoptes Quick Start: Spectro Cloud Palette Deployment

Deploy Panoptes security monitoring to your Kubernetes clusters via Spectro Cloud Palette.

---

## Prerequisites

```bash
# Spectro Cloud account with Palette access
# https://console.spectrocloud.com

# At least one managed Kubernetes cluster
# (EKS, AKS, GKE, or on-prem)

# Cluster requirements:
# - Kubernetes 1.28+
# - Linux nodes (inotify/fanotify kernel features)
# - containerd or CRI-O runtime
```

---

## 1. Choose Your Pack

| Pack | Use Case | Components |
|------|----------|------------|
| **panoptes** | Full security suite | Argus + Janus + Dashboard |
| **argus-fim** | File integrity only | Argus operator + daemon |
| **janus-audit** | Access auditing only | Janus operator + daemon |

For most deployments, use the **panoptes** meta-pack.

---

## 2. Choose Your Preset

| Preset | Description | Best For |
|--------|-------------|----------|
| **default** | Standard deployment, all components | General use |
| **compliance** | PCI-DSS/SOC2 optimized, extended retention | Regulated environments |
| **minimal** | Argus only, no UI | Resource-constrained clusters |

---

## 3. Add Pack to Cluster Profile

### Via Palette UI

1. Navigate to **Profiles** → **Cluster Profiles**
2. Click **Add New Cluster Profile** (or edit existing)
3. Add a new **Add-on Layer**
4. Search for `panoptes` in the pack registry
5. Select version `2.0.0`
6. Choose preset or customize values

### Via Palette API

```bash
# Create profile with Panoptes pack
curl -X POST "https://api.spectrocloud.com/v1/clusterprofiles" \
  -H "Authorization: Bearer $PALETTE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "name": "production-secure"
    },
    "spec": {
      "packs": [
        {
          "name": "panoptes",
          "version": "2.0.0",
          "values": "# Use default preset"
        }
      ]
    }
  }'
```

---

## 4. Configure Pack Values

### Default Configuration

```yaml
pack:
  namespace: panoptes-system

components:
  argus:
    enabled: true
  janus:
    enabled: true
  dashboard:
    enabled: true

# Image registry (use Spectro Cloud registry)
global:
  imageRegistry: gcr.io/spectro-images

# Argus FIM settings
argus:
  controller:
    replicas: 1
    resources:
      requests:
        cpu: 100m
        memory: 128Mi
  daemon:
    resources:
      requests:
        cpu: 50m
        memory: 64Mi

# Janus Audit settings
janus:
  controller:
    replicas: 1
  enforcement:
    enabled: true

# Dashboard settings
dashboard:
  replicas: 1
  service:
    type: ClusterIP
    port: 3000
  ingress:
    enabled: false
```

### Compliance Preset (PCI-DSS/SOC2)

```yaml
# Use compliance preset for regulated environments
pack:
  preset: compliance

# Extended event retention (13 months for PCI-DSS)
observability:
  retention:
    events: 13mo
    metrics: 13mo

# Critical path monitoring
argus:
  defaultWatchers:
    enabled: true
    paths:
      - /etc/passwd
      - /etc/shadow
      - /etc/sudoers
      - /usr/bin
      - /usr/sbin

# Audit logging enabled
janus:
  audit:
    kernelAudit: true

# Prometheus integration
observability:
  prometheus:
    enabled: true
    serviceMonitor:
      enabled: true
```

### Minimal Preset (Resource-Constrained)

```yaml
# Use minimal preset for lightweight deployment
pack:
  preset: minimal

components:
  argus:
    enabled: true
  janus:
    enabled: false  # Disabled
  dashboard:
    enabled: false  # Disabled

argus:
  controller:
    resources:
      requests:
        cpu: 50m
        memory: 64Mi
      limits:
        cpu: 200m
        memory: 128Mi
```

---

## 5. Deploy to Cluster

### New Cluster

1. Navigate to **Clusters** → **Add New Cluster**
2. Select infrastructure provider (AWS, Azure, GCP, etc.)
3. Select cluster profile with Panoptes pack
4. Configure cluster settings
5. Deploy

### Existing Cluster

1. Navigate to **Clusters** → Select your cluster
2. Click **Profile** tab
3. Click **Attach Add-on Profile**
4. Select profile containing Panoptes pack
5. Apply changes

---

## 6. Verify Deployment

### Check Pod Status

```bash
# Set context to managed cluster
kubectl config use-context <cluster-context>

# Verify pods are running
kubectl get pods -n panoptes-system

# Expected output:
# NAME                                READY   STATUS    RESTARTS   AGE
# argus-operator-6d9b8c7f5-xxxxx     1/1     Running   0          5m
# argusd-xxxxx                        1/1     Running   0          5m
# janus-operator-7c8d9e6f4-xxxxx     1/1     Running   0          5m
# janusd-xxxxx                        1/1     Running   0          5m
# panoptes-eye-5f4e3d2c1-xxxxx       1/1     Running   0          5m
```

### Check CRDs

```bash
kubectl get crd | grep -E "argus|janus"

# Expected:
# arguswatchers.argus.como-technologies.io
# janusguards.janus.como-technologies.io
```

---

## 7. Create Test Resources

### ArgusWatcher (File Integrity)

```yaml
kubectl apply -f - <<EOF
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: critical-files
  namespace: default
spec:
  selector:
    matchLabels:
      security.panoptes.io/monitored: "true"
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/hosts
      events:
        - modify
        - create
        - delete
      tags:
        severity: high
        compliance: pci-dss
EOF
```

### JanusGuard (Access Auditing)

```yaml
kubectl apply -f - <<EOF
apiVersion: janus.como-technologies.io/v1
kind: JanusGuard
metadata:
  name: sensitive-access
  namespace: default
spec:
  selector:
    matchLabels:
      security.panoptes.io/monitored: "true"
  subjects:
    - deny:
        - /etc/shadow
      events:
        - access
        - open
      audit: true
      enforcing: false  # Dry-run mode
      tags:
        severity: critical
EOF
```

---

## 8. Access Dashboard

### Option A: Port Forward (Development)

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000 &
open http://localhost:3000
```

### Option B: Ingress (Production)

Update pack values to enable ingress:

```yaml
dashboard:
  ingress:
    enabled: true
    className: nginx  # or your ingress class
    host: panoptes.your-domain.com
    tls:
      enabled: true
      secretName: panoptes-tls
```

### Option C: Palette Virtual Cluster Gateway

Use Palette's built-in gateway for secure access without public ingress.

---

## 9. Multi-Cluster Monitoring

### Cluster Identification

Each cluster needs a unique identifier for multi-cluster aggregation. Configure this in your pack values:

```yaml
# Per-cluster values override
global:
  cluster:
    name: "prod-east-1"        # Unique cluster identifier
    environment: "production"
    region: "us-east-1"
```

### Palette Macro Auto-Injection

Use Spectro Cloud Palette system macros to automatically inject cluster identity:

```yaml
# Automatic cluster naming via Palette macros
global:
  cluster:
    name: "{{ .spectro.system.cluster.name }}"
    # Or combine with project: "{{ .spectro.system.project.name }}-{{ .spectro.system.cluster.name }}"
```

Available Palette macros:
- `{{ .spectro.system.cluster.name }}` - Cluster name from Palette
- `{{ .spectro.system.cluster.uid }}` - Unique cluster UID
- `{{ .spectro.system.project.name }}` - Project name

### Central Observability Pattern

Deploy a central monitoring cluster with Prometheus/Grafana, then configure each Panoptes deployment to export metrics:

```yaml
observability:
  opentelemetry:
    enabled: true
    endpoint: "otel-collector.central-monitoring:4317"

  prometheus:
    remoteWrite:
      enabled: true
      url: "https://prometheus.central-monitoring/api/v1/write"
```

### Cluster Labeling

All metrics and events are automatically labeled with cluster identity:

```promql
# Query events across all clusters
sum by (cluster) (
  increase(argus_events_total{severity="critical"}[5m])
)

# Filter to specific cluster
argus_events_total{cluster="prod-east-1"}
```

For detailed multi-cluster setup including Prometheus federation, see [Multi-Cluster Guide](guides/multi-cluster.md).

---

## 10. Troubleshooting

### Pack Not Deploying

```bash
# Check Palette agent logs
kubectl logs -n palette-system -l app=palette-agent

# Check Helm release status
helm list -n panoptes-system
helm history panoptes -n panoptes-system
```

### Daemons Not Starting

```bash
# Check DaemonSet status
kubectl describe daemonset -n panoptes-system argusd

# Common issues:
# - Node doesn't have containerd/CRI-O
# - Insufficient capabilities (SYS_ADMIN required)
# - hostPID not enabled
```

### No Events Detected

```bash
# Verify watcher is targeting pods
kubectl get arguswatcher critical-files -o jsonpath='{.status}'

# Check daemon logs
kubectl logs -n panoptes-system -l app=argusd

# Ensure pods have the correct label
kubectl get pods -l security.panoptes.io/monitored=true
```

### Metrics Not Appearing

```bash
# Check ServiceMonitor
kubectl get servicemonitor -n panoptes-system

# Verify Prometheus is scraping
kubectl port-forward -n monitoring svc/prometheus 9090:9090
# Navigate to Status → Targets
```

---

## Quick Reference

| Action | Command/Location |
|--------|------------------|
| View packs | Palette → Settings → Pack Registry |
| Add to profile | Palette → Profiles → Add Layer |
| Deploy cluster | Palette → Clusters → Add New Cluster |
| Check status | `kubectl get pods -n panoptes-system` |
| View watchers | `kubectl get aw` (ArgusWatchers) |
| View guards | `kubectl get jg` (JanusGuards) |
| Access UI | `kubectl port-forward svc/panoptes-eye 3000:3000` |
| View metrics | Grafana dashboard or Prometheus |

---

## Pack Versions

| Pack | Version | K8s Compatibility |
|------|---------|-------------------|
| panoptes | 2.0.0 | 1.28+ |
| argus-fim | 2.0.0 | 1.28+ |
| janus-audit | 2.0.0 | 1.28+ |

---

## Next Steps

1. **Configure alerts**: Add PrometheusRules for critical file events
2. **Enable compliance reporting**: Use compliance preset for PCI-DSS/SOC2
3. **Set up multi-cluster**: Configure central observability stack
4. **Customize watchers**: Create organization-specific ArgusWatcher templates
5. **Review FUTURE_STATE.md**: See planned features and roadmap

---

## Related Documentation

- [Local Development Guide](QUICK_START.md) - Test locally with kind
- [Future State & Roadmap](FUTURE_STATE.md) - Planned features
- [Argus FIM Pack](../packs/argus-fim/README.md) - Standalone FIM
- [Janus Audit Pack](../packs/janus-audit/README.md) - Standalone audit

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
