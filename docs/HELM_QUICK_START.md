# Helm Quickstart: Panoptes in 5 Minutes

Get Panoptes running on any Kubernetes cluster with a single Helm command.

## Prerequisites

- Kubernetes 1.28+ cluster (Kind, EKS, GKE, AKS, etc.)
- Helm 3.14+
- `kubectl` configured for your cluster

## Install

```bash
# Install the full Panoptes suite with PCI-DSS compliance monitoring
helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
  --namespace panoptes-system \
  --create-namespace \
  --set compliance.pciDss.enabled=true
```

This deploys:
- **Argus** operator + daemon (file integrity monitoring via inotify)
- **Janus** operator + daemon (file access auditing via fanotify)
- **Panoptes Eye** dashboard
- **PCI-DSS** compliance templates (ArgusWatcher + JanusGuard for PCI-DSS 10.5.5, 11.5, 7.1)

## Verify

```bash
# Check all pods are running
kubectl get pods -n panoptes-system

# See the compliance resources created
kubectl get arguswatchers,janusguards -n panoptes-system
```

Expected output:
```
NAME                                              AGE
arguswatcher.argus.como-technologies.io/panoptes-pci-dss-fim   30s

NAME                                              AGE
janusguard.janus.como-technologies.io/panoptes-pci-dss-access  30s
```

## Try It: Detect a Compliance Violation

```bash
# Deploy a test workload labeled for PCI-DSS monitoring
kubectl run payment-service --image=nginx:alpine \
  --labels="pci-dss/scope=in-scope" \
  -n panoptes-system

# Wait for it to start
kubectl wait --for=condition=Ready pod/payment-service -n panoptes-system --timeout=60s

# Simulate a compliance violation: modify /etc/passwd
kubectl exec payment-service -n panoptes-system -- sh -c "echo 'backdoor:x:0:0::/root:/bin/sh' >> /etc/passwd"

# Check Argus detected the modification
kubectl logs -n panoptes-system -l app.kubernetes.io/component=controller --tail=20
```

You should see a log entry showing the file modification event tagged with `requirement: "11.5"` and `severity: critical`.

## Open the Dashboard

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:80
# Open http://localhost:3000
```

## Enable Additional Compliance Frameworks

```bash
# Enable HIPAA monitoring
helm upgrade panoptes oci://ghcr.io/como-technologies/charts/panoptes \
  --namespace panoptes-system \
  --set compliance.pciDss.enabled=true \
  --set compliance.hipaa.enabled=true

# Enable all frameworks
helm upgrade panoptes oci://ghcr.io/como-technologies/charts/panoptes \
  --namespace panoptes-system \
  --set compliance.pciDss.enabled=true \
  --set compliance.hipaa.enabled=true \
  --set compliance.soc2.enabled=true \
  --set compliance.cisKubernetes.enabled=true \
  --set compliance.nist80053.enabled=true \
  --set compliance.gdpr.enabled=true
```

Available frameworks:

| Framework | Value Key | Pod Label |
|-----------|-----------|-----------|
| PCI-DSS 4.0 | `compliance.pciDss` | `pci-dss/scope: in-scope` |
| HIPAA | `compliance.hipaa` | `hipaa/scope: ephi` |
| SOC2 | `compliance.soc2` | `soc2/scope: in-scope` |
| CIS Kubernetes | `compliance.cisKubernetes` | `cis/scope: kubernetes-audit` |
| NIST 800-53 | `compliance.nist80053` | `nist-800-53/scope: moderate` |
| GDPR | `compliance.gdpr` | `gdpr/scope: personal-data` |
| Base Security | `compliance.baseSecurity` | `panoptes.como-technologies.io/monitored: "true"` |

## Install Standalone Components

If you only need FIM (Argus) or access control (Janus):

```bash
# Argus only (file integrity monitoring)
helm install argus oci://ghcr.io/como-technologies/charts/panoptes-argus \
  --namespace panoptes-system --create-namespace

# Janus only (file access auditing)
helm install janus oci://ghcr.io/como-technologies/charts/panoptes-janus \
  --namespace panoptes-system --create-namespace
```

## Uninstall

```bash
helm uninstall panoptes -n panoptes-system
kubectl delete namespace panoptes-system
```

## Next Steps

- [Security demo scenarios](../examples/README.md) — Copy-paste runnable demos
- [Compliance monitoring guide](guides/quickstart-compliance.md) — Deep dive into compliance templates
- [What to monitor](guides/what-to-monitor.md) — Recommended paths and strategies
- [Kernel tuning](operations/kernel-tuning.md) — Optimize inotify/fanotify limits for production
- [Monitoring & alerting](guides/monitoring-alerting.md) — Prometheus, Grafana, AlertManager integration
