# Configuration Drift Detection

> **Time:** 5 min (quick start) | 30+ min (deep dive)

Detect unauthorized or accidental changes to application and system configuration files.

## Problem Statement

### The Challenge

Configuration drift occurs when systems deviate from their intended state. In Kubernetes, this can happen when:
- Developers SSH into pods and make manual changes
- Applications modify their own config files unexpectedly
- ConfigMap/Secret updates don't propagate correctly
- Attackers modify configurations to establish persistence

### Who Needs This

- **Platform engineers** ensuring GitOps consistency
- **Security teams** detecting unauthorized changes
- **SREs** troubleshooting configuration-related incidents
- **DevOps teams** validating deployment correctness

### Use Cases

- Detect manual changes in GitOps environments
- Monitor application configuration directories
- Track Kubernetes ConfigMap/Secret mount changes
- Identify potential security compromises via config modifications

---

## Quick Start (5 Minutes)

### Step 1: Label Your Pods (30 seconds)

```bash
# Label pods to monitor for configuration drift
kubectl label pods -l app=my-app config-drift/monitored=true
```

### Step 2: Apply the Configuration (30 seconds)

```bash
kubectl apply -f - <<'EOF'
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: config-drift-detector
  labels:
    use-case: config-drift
spec:
  selector:
    matchLabels:
      config-drift/monitored: "true"
  subjects:
    # System configuration
    - paths:
        - /etc
      events:
        - modify
        - create
        - delete
        - attrib
      recursive: true
      maxDepth: 3
      ignore:
        - "*.swp"
        - "*~"
        - "*.tmp"
      tags:
        category: system-config
        severity: high

    # Application configuration
    - paths:
        - /app/config
        - /opt/app/config
        - /home/app/.config
      events:
        - modify
        - create
        - delete
      recursive: true
      tags:
        category: app-config
        severity: medium

    # Kubernetes mounted configs
    - paths:
        - /etc/config
        - /etc/secrets
      events:
        - modify
        - create
        - delete
      recursive: true
      tags:
        category: k8s-config
        severity: high

  containerRuntime: auto
  paused: false
  logFormat: json
EOF
```

### Step 3: Verify It's Working (2 minutes)

```bash
# Check watcher status
kubectl get arguswatcher config-drift-detector -o wide

# Generate a test event
POD=$(kubectl get pods -l config-drift/monitored=true -o jsonpath='{.items[0].metadata.name}')
kubectl exec $POD -- touch /etc/test-drift
kubectl exec $POD -- rm /etc/test-drift
```

### Step 4: View in Dashboard (1 minute)

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

Navigate to **Events** page and filter by tag `category: system-config`

---

## What Success Looks Like

### Expected Events

| Event Type | Path | Category | Meaning |
|------------|------|----------|---------|
| `modify` | `/etc/nginx/nginx.conf` | system-config | Config file changed |
| `create` | `/app/config/override.yaml` | app-config | New config file created |
| `delete` | `/etc/cron.d/cleanup` | system-config | Scheduled task removed |
| `attrib` | `/etc/ssh/sshd_config` | system-config | Permissions changed |

### What to Investigate

- **Any changes in GitOps environments** - Should only change via deployment
- **Unexpected `/etc` modifications** - Could indicate manual access or compromise
- **New files in config directories** - Potential persistence mechanism
- **Permission changes** - May indicate privilege escalation attempt

---

## Deep Dive

### Workload-Specific Configuration

#### Web Servers (nginx, Apache)

```yaml
subjects:
  - paths:
      - /etc/nginx
      - /etc/apache2
      - /etc/httpd
    events: [modify, create, delete]
    recursive: true
    tags:
      workload: web-server
```

#### Databases (PostgreSQL, MySQL)

```yaml
subjects:
  - paths:
      - /var/lib/postgresql/data/*.conf
      - /etc/postgresql
      - /etc/mysql
    events: [modify, attrib]
    tags:
      workload: database
      severity: critical
```

#### Message Queues (Kafka, RabbitMQ)

```yaml
subjects:
  - paths:
      - /opt/kafka/config
      - /etc/rabbitmq
    events: [modify, create, delete]
    recursive: true
    tags:
      workload: message-queue
```

### GitOps Integration

In GitOps environments, ANY configuration change outside the deployment pipeline indicates drift:

```yaml
subjects:
  # Alert on ALL /etc changes - nothing should change manually
  - paths:
      - /etc
    events:
      - modify
      - create
      - delete
    recursive: true
    ignore:
      - "resolv.conf"      # Dynamic DNS
      - "hosts"            # Dynamic hosts
      - "mtab"             # Mount table
      - "*.lock"           # Lock files
    tags:
      gitops: "drift-detected"
      severity: critical
```

### Kubernetes ConfigMap/Secret Monitoring

Monitor paths where ConfigMaps and Secrets are mounted:

```yaml
subjects:
  # ConfigMap mounts (typical paths)
  - paths:
      - /etc/config
      - /app/config
    events:
      - modify
      - create
      - delete
    tags:
      source: configmap

  # Secret mounts
  - paths:
      - /etc/secrets
      - /var/run/secrets
    events:
      - modify
      - create
      - delete
    ignore:
      - "kubernetes.io"  # Service account tokens
    tags:
      source: secret
      severity: critical
```

### Baseline Comparison

For environments where you need to compare against a known-good baseline:

1. **Capture baseline** (during deployment):
   ```bash
   kubectl exec $POD -- find /etc -type f -exec sha256sum {} \; > baseline.txt
   ```

2. **Compare current state**:
   ```bash
   kubectl exec $POD -- find /etc -type f -exec sha256sum {} \; > current.txt
   diff baseline.txt current.txt
   ```

3. **Use events for ongoing monitoring** - ArgusWatcher detects changes as they happen

### Alerting Configuration

```yaml
groups:
  - name: panoptes-config-drift
    rules:
      - alert: ConfigurationDriftDetected
        expr: increase(panoptes_argus_events_total{tags_category="system-config"}[5m]) > 0
        for: 0m
        labels:
          severity: warning
        annotations:
          summary: "Configuration drift detected"
          description: "File {{ $labels.path }} was {{ $labels.event_type }} on {{ $labels.pod }}"

      - alert: CriticalConfigChanged
        expr: increase(panoptes_argus_events_total{tags_severity="critical", tags_category=~".*config.*"}[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Critical configuration file changed"
```

### Operational Considerations

#### Expected vs. Unexpected Changes

| Change Type | Expected? | Action |
|-------------|-----------|--------|
| Deployment updates ConfigMap | Yes | Correlate with deployment events |
| Manual kubectl exec edit | No | Investigate, revert if needed |
| Application self-modifying | Maybe | Review application behavior |
| Init container setup | Yes | Happens once at startup |

#### Event Volume

- **Low traffic**: 5-20 events/day for stable workloads
- **High traffic**: 100+ events/day during active development

#### Reducing Noise

```yaml
ignore:
  - "*.swp"          # Vim swap files
  - "*~"             # Backup files
  - "*.tmp"          # Temporary files
  - "*.pid"          # PID files
  - "*.sock"         # Socket files
  - "lost+found"     # Filesystem recovery
```

---

## Troubleshooting

### Changes Not Detected

1. **Verify path exists in container:**
   ```bash
   kubectl exec $POD -- ls -la /etc
   ```

2. **Check maxDepth setting** - may need to increase for nested configs

3. **Verify container runtime detection:**
   ```bash
   kubectl logs -n panoptes-system -l app.kubernetes.io/name=argusd | grep "runtime"
   ```

### Too Many Events from Expected Changes

1. Add specific paths to `ignore` list
2. Reduce `maxDepth` if recursion is too deep
3. Filter events by `category` tag in dashboard

---

## Related Documentation

- [What to Monitor](../what-to-monitor.md) - Workload-specific path recommendations
- [Security Incident Detection](security-incident-detection.md) - Detecting malicious changes
- [Compliance Monitoring](compliance-monitoring.md) - Compliance-driven change detection
