# Panoptes Compliance Templates

Ready-to-deploy security monitoring configurations for common compliance frameworks.

## Available Templates

| Template | Framework | Description |
|----------|-----------|-------------|
| [pci-dss.yaml](pci-dss.yaml) | PCI-DSS 3.2.1/4.0 | Payment card industry data security |
| [hipaa.yaml](hipaa.yaml) | HIPAA Security Rule | Healthcare protected health information |
| [soc2.yaml](soc2.yaml) | SOC 2 Type II | Service organization trust criteria |
| [cis-kubernetes.yaml](cis-kubernetes.yaml) | CIS Kubernetes v1.8 | Kubernetes security hardening |
| [base-security.yaml](base-security.yaml) | General | Minimal security baseline for any workload |

## Quick Start

### 1. Choose a Template

Start with `base-security.yaml` for general workloads, or select a compliance-specific template.

### 2. Label Your Pods

Each template uses a specific label selector. Label your pods accordingly:

```bash
# For base-security template
kubectl label pod <pod-name> security.panoptes.io/monitored=true

# For PCI-DSS template
kubectl label pod <pod-name> pci-dss/scope=in-scope

# For HIPAA template
kubectl label pod <pod-name> hipaa/scope=ephi

# For SOC 2 template
kubectl label pod <pod-name> soc2/scope=in-scope

# For CIS Kubernetes template
kubectl label pod <pod-name> cis/scope=kubernetes-audit
```

### 3. Apply the Template

```bash
# Apply a specific template
kubectl apply -f base-security.yaml

# Or apply all compliance templates
kubectl apply -f .
```

### 4. Verify Deployment

```bash
# Check ArgusWatchers
kubectl get arguswatchers -l panoptes.io/template

# Check JanusGuards
kubectl get janusguards -l panoptes.io/template

# View status
kubectl describe arguswatcher base-security-fim
```

## Template Customization

### Modifying Paths

Each template monitors paths relevant to its compliance framework. To add custom paths:

```yaml
spec:
  subjects:
    # Add your custom paths
    - paths:
        - /app/custom/config
        - /data/sensitive
      events:
        - modify
        - create
        - delete
      tags:
        severity: high
        category: custom
```

### Changing Label Selectors

To monitor different pods, change the `selector`:

```yaml
spec:
  selector:
    matchLabels:
      your-label: "your-value"
```

### Enabling Enforcement

JanusGuard templates start in audit-only mode (`enforcing: false`). After validating no false positives, enable enforcement:

```yaml
spec:
  enforcing: true
```

Or patch an existing guard:

```bash
kubectl patch janusguard pci-dss-access --type=merge -p '{"spec":{"enforcing":true}}'
```

## Compliance Requirements Mapping

### PCI-DSS

| Requirement | Coverage | Resource |
|-------------|----------|----------|
| 7.1 | Access control enforcement | JanusGuard |
| 10.2 | Audit trail logging | JanusGuard |
| 10.5.5 | FIM on log files | ArgusWatcher |
| 10.6 | Security alert review | Both |
| 11.5 | Change detection | ArgusWatcher |

### HIPAA

| Requirement | Coverage | Resource |
|-------------|----------|----------|
| 164.312(b) | Audit controls | JanusGuard |
| 164.312(c)(1) | Data integrity | ArgusWatcher |
| 164.312(d) | Authentication | JanusGuard |
| 164.308(a)(1) | Security management | Both |

### SOC 2

| Criteria | Coverage | Resource |
|----------|----------|----------|
| CC6.1 | Logical access security | JanusGuard |
| CC6.2 | Access authorization | JanusGuard |
| CC7.1 | System operations | ArgusWatcher |
| CC7.2 | System monitoring | ArgusWatcher |
| CC7.3 | Incident detection | JanusGuard |

### CIS Kubernetes

| Section | Coverage | Resource |
|---------|----------|----------|
| 1.1.x | Control plane config | ArgusWatcher |
| 4.1.x | Worker node config | ArgusWatcher |
| 4.2.x | Kubelet configuration | ArgusWatcher |
| 5.1.x | Service accounts | JanusGuard |
| 5.4.x | Pod security policies | JanusGuard |

## Combining Templates

You can apply multiple templates to the same pods using multiple labels:

```bash
# Pod needs both PCI-DSS and SOC 2 compliance
kubectl label pod payment-api pci-dss/scope=in-scope soc2/scope=in-scope
```

Both `pci-dss.yaml` and `soc2.yaml` ArgusWatchers/JanusGuards will monitor the pod.

## Verifying Compliance

After deploying templates, check the Panoptes Eye dashboard:

1. Navigate to **Compliance** page
2. View framework scores
3. Expand frameworks to see individual check status
4. Address any failing checks

## Troubleshooting

### No Events Appearing

1. Verify pods have correct labels:
   ```bash
   kubectl get pods -l security.panoptes.io/monitored=true
   ```

2. Check ArgusWatcher status:
   ```bash
   kubectl describe arguswatcher base-security-fim
   ```

3. Ensure daemons are running:
   ```bash
   kubectl get pods -n panoptes-system
   ```

### Too Many Events

Adjust `ignore` patterns in ArgusWatcher subjects to filter noise:

```yaml
subjects:
  - paths:
      - /var/log
    ignore:
      - "*.gz"
      - "*.tmp"
      - "debug.*"
```

## Support

- [Panoptes Documentation](../README.md)
- [Monitoring Guide](../guides/what-to-monitor.md)
- [GitHub Issues](https://github.com/your-org/panoptes/issues)
