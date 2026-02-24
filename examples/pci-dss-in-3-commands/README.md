# PCI-DSS Compliance Monitoring in 3 Commands

The hero demo. Get PCI-DSS file integrity monitoring and access auditing running in under 2 minutes.

## What This Demonstrates

- ArgusWatcher detecting modifications to critical system files (PCI-DSS 11.5)
- ArgusWatcher monitoring log file integrity (PCI-DSS 10.5.5)
- JanusGuard auditing access to sensitive credential files (PCI-DSS 7.1)
- Real-time event detection via kernel inotify and fanotify

## The 3 Commands

```bash
# 1. Install Panoptes with PCI-DSS compliance enabled
helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
  -n panoptes-system --create-namespace \
  --set compliance.pciDss.enabled=true

# 2. Deploy a payment service workload
kubectl apply -f workload.yaml

# 3. Simulate violations and observe detections
kubectl exec deploy/payment-service -- sh -c "echo 'hacked' >> /etc/passwd"
kubectl get aw pci-dss-fim -o wide
```

## Automated Demo

Run the full automated walkthrough:

```bash
./demo.sh
```

Clean up when done:

```bash
./demo.sh --cleanup
```

## Step-by-Step Walkthrough

### Step 1: Install Panoptes

The Helm chart deploys the Argus operator, Janus operator, argusd DaemonSet, and janusd DaemonSet. The `compliance.pciDss.enabled=true` flag automatically creates PCI-DSS ArgusWatcher and JanusGuard resources.

```bash
helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
  -n panoptes-system --create-namespace \
  --set compliance.pciDss.enabled=true
```

Expected output:
```
NAME: panoptes
NAMESPACE: panoptes-system
STATUS: deployed
```

### Step 2: Deploy the Payment Service

This deploys an nginx-based pod labeled `pci-dss/scope: in-scope`, which the PCI-DSS ArgusWatcher and JanusGuard automatically target via label selectors.

```bash
kubectl apply -f workload.yaml
kubectl wait --for=condition=available deploy/payment-service --timeout=120s
```

Expected output:
```
deployment.apps/payment-service created
deployment.apps/payment-service condition met
```

### Step 3: Simulate PCI-DSS Violations

Simulate common attack patterns that PCI-DSS monitoring detects:

```bash
# Modify /etc/passwd (PCI-DSS 11.5 - Critical system file change)
POD=$(kubectl get pods -l app=payment-service -o jsonpath='{.items[0].metadata.name}')
kubectl exec "$POD" -- sh -c "echo 'backdoor:x:0:0::/root:/bin/sh' >> /etc/passwd"

# Attempt to read /etc/shadow (PCI-DSS 7.1 - Unauthorized credential access)
kubectl exec "$POD" -- cat /etc/shadow

# Delete a log file (PCI-DSS 10.5.5 - Log tampering)
kubectl exec "$POD" -- rm -f /var/log/dpkg.log
```

### Step 4: Observe Detections

```bash
# Check ArgusWatcher status
kubectl get aw

# View argus operator logs for FIM events
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argus-operator --tail=20

# View argusd daemon logs for kernel-level detections
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argusd --tail=20
```

Expected output (abbreviated):
```
NAME           AGE   STATUS
pci-dss-fim    2m    Active

# In the logs you will see events like:
# {"level":"info","event":"modify","path":"/etc/passwd","requirement":"11.5","severity":"critical"}
# {"level":"info","event":"delete","path":"/var/log/dpkg.log","requirement":"10.5.5","severity":"high"}
```

## Files

| File | Description |
|------|-------------|
| `workload.yaml` | Payment service Deployment with PCI-DSS labels |
| `demo.sh` | Automated demo script with cleanup support |

## What to Try Next

- Enable enforcement mode: `kubectl patch jg pci-dss-access -p '{"spec":{"enforcing":true}}'`
- Open the Panoptes Eye dashboard: `kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000`
- Apply the full compliance template: `kubectl apply -f deploy/compliance/pci-dss/template.yaml`
