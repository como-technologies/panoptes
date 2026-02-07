# Quick Start: 5-Minute Security Monitoring

Get basic security monitoring running in your Kubernetes cluster in under 5 minutes.

## Prerequisites

- Panoptes operators deployed (see [QUICK_START.md](../QUICK_START.md))
- `kubectl` configured for your cluster
- At least one workload pod to monitor

## Step 1: Label Your Pods (30 seconds)

Label the pods you want to monitor:

```bash
# Label a specific pod
kubectl label pod my-app-pod panoptes.como-technologies.io/monitored=true

# Label all pods in a deployment
kubectl label pods -l app=my-app panoptes.como-technologies.io/monitored=true

# Label all pods in a namespace
kubectl label pods -n production --all panoptes.como-technologies.io/monitored=true
```

## Step 2: Apply Base Security Template (30 seconds)

```bash
# Apply the base security template
kubectl apply -k deploy/compliance/base-security/
```

Or create it directly:

```bash
kubectl apply -f - <<'EOF'
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: quick-start-fim
spec:
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/sudoers
      events: [modify, delete, attrib]
      tags:
        severity: critical
    - paths:
        - /etc/ssh
        - /root/.ssh
      events: [all]
      recursive: true
    - paths:
        - /var/log
      events: [delete]
      recursive: true
      maxDepth: 2
---
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: quick-start-access
spec:
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"
  subjects:
    - deny:
        - /etc/shadow
        - /root/.ssh/id_*
      events: [access, open]
      audit: true
    - deny:
        - /var/run/secrets/kubernetes.io
      events: [access, open]
      autoAllowOwner: true
      audit: true
  enforcing: false
EOF
```

## Step 3: Verify Deployment (1 minute)

Check that the resources were created:

```bash
# List ArgusWatchers
kubectl get arguswatchers
# NAME              AGE
# quick-start-fim   10s

# List JanusGuards
kubectl get janusguards
# NAME                  AGE
# quick-start-access    10s

# Check status
kubectl describe arguswatcher quick-start-fim
```

## Step 4: Access the Dashboard (2 minutes)

```bash
# Port-forward the dashboard
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000 &

# Open in browser
open http://localhost:3000
```

Navigate to:
- **Events** - See real-time file system events
- **Compliance** - View security compliance scores
- **Watchers/Guards** - Manage your monitoring rules

## Step 5: Generate a Test Event (1 minute)

Trigger a test event to verify monitoring is working:

```bash
# Get a monitored pod name
POD=$(kubectl get pods -l panoptes.como-technologies.io/monitored=true -o jsonpath='{.items[0].metadata.name}')

# Exec into the pod and touch a monitored file
kubectl exec $POD -- touch /etc/test-file

# Check the Events page in the dashboard - you should see a "create" event
```

## You're Done!

Your cluster now has basic security monitoring for:
- User account changes (`/etc/passwd`, `/etc/shadow`)
- SSH configuration modifications
- Privilege escalation attempts (`/etc/sudoers`)
- Log tampering detection
- Kubernetes secrets access auditing

## Next Steps

### Expand Monitoring

```bash
# Add more pods to monitoring
kubectl label pods -l app=database panoptes.como-technologies.io/monitored=true
```

### Add Compliance Templates

```bash
# Apply PCI-DSS monitoring
kubectl apply -k deploy/compliance/pci-dss/
kubectl label pods -l app=payment pci-dss/scope=in-scope
```

### Enable Enforcement

After validating there are no false positives, enable enforcement on JanusGuards:

```bash
kubectl patch janusguard quick-start-access --type=merge -p '{"spec":{"enforcing":true}}'
```

### Set Up Alerts

Configure Prometheus alerts for critical events (see [alerting documentation](./monitoring-alerting.md#alerting-rules)).

## Troubleshooting

### No Events Appearing

1. Check pods are labeled:
   ```bash
   kubectl get pods -l panoptes.como-technologies.io/monitored=true
   ```

2. Check daemon pods are running:
   ```bash
   kubectl get pods -n panoptes-system
   ```

3. Check ArgusWatcher status:
   ```bash
   kubectl describe arguswatcher quick-start-fim
   ```

### Too Many Events

Add ignore patterns for noisy paths:

```bash
kubectl patch arguswatcher quick-start-fim --type=json -p='[
  {"op": "add", "path": "/spec/subjects/2/ignore", "value": ["*.gz", "*.tmp"]}
]'
```

## Quick Reference

| Command | Description |
|---------|-------------|
| `kubectl get aw` | List ArgusWatchers |
| `kubectl get jg` | List JanusGuards |
| `kubectl describe aw <name>` | View ArgusWatcher details |
| `kubectl logs -n panoptes-system -l app=argusd` | View daemon logs |

## Related Documentation

- [What to Monitor](./what-to-monitor.md) - Detailed monitoring guidance
- [Compliance Templates](../../deploy/compliance/) - Framework-specific templates
- [Full Quick Start](../QUICK_START.md) - Complete deployment guide
