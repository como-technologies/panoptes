# Panoptes Use Case Guides

Practical, drop-in ready guides for common security monitoring scenarios.

## Available Use Cases

| Guide | Time | Description |
|-------|------|-------------|
| [Compliance Monitoring](compliance-monitoring.md) | 5 min | PCI-DSS, HIPAA, SOC 2 compliance with file integrity monitoring |
| [Configuration Drift Detection](configuration-drift-detection.md) | 5 min | Detect unauthorized changes to application and system configs |
| [Security Incident Detection](security-incident-detection.md) | 5 min | Detect persistence mechanisms, privilege escalation, credential theft |
| [Audit Logging](audit-logging.md) | 5 min | Comprehensive file access logging for regulated environments |

## Pod Label Quick Reference

Every compliance framework requires labeling pods to identify which workloads to monitor:

| Framework | Label | Value | kubectl command |
|-----------|-------|-------|-----------------|
| Base Security | `panoptes.como-technologies.io/monitored` | `true` | `kubectl label pod NAME panoptes.como-technologies.io/monitored=true` |
| PCI-DSS | `pci-dss/scope` | `in-scope` | `kubectl label pod NAME pci-dss/scope=in-scope` |
| HIPAA | `hipaa/scope` | `ephi` | `kubectl label pod NAME hipaa/scope=ephi` |
| SOC 2 | `soc2/scope` | `in-scope` | `kubectl label pod NAME soc2/scope=in-scope` |
| CIS Kubernetes | `cis/scope` | `kubernetes-audit` | `kubectl label pod NAME cis/scope=kubernetes-audit` |
| NIST 800-53 | `nist-800-53/scope` | `moderate` | `kubectl label pod NAME nist-800-53/scope=moderate` |
| GDPR | `gdpr/scope` | `personal-data` | `kubectl label pod NAME gdpr/scope=personal-data` |

### Multi-Framework Compliance

Apply multiple labels for workloads that need to meet multiple compliance frameworks:

```bash
kubectl label pod myapp-pod \
  pci-dss/scope=in-scope \
  soc2/scope=in-scope \
  hipaa/scope=ephi
```

## Prerequisites

Before following any use case guide, ensure:

1. **Panoptes operators are deployed:**
   ```bash
   kubectl get pods -n panoptes-system
   # Should show: argus-operator, janus-operator, argusd, janusd
   ```

2. **CRDs are registered:**
   ```bash
   kubectl get crd arguswatchers.argus.como-technologies.io
   kubectl get crd janusguards.janus.como-technologies.io
   ```

3. **Dashboard is accessible:**
   ```bash
   kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
   # Open http://localhost:3000
   ```

## Guide Structure

Each use case guide follows the same structure:

1. **Problem Statement** - What challenge this solves
2. **Quick Start (5 min)** - Copy-paste commands to get working immediately
3. **What Success Looks Like** - Expected events and dashboard state
4. **Deep Dive** - Advanced configuration, alerting, and operational guidance

## Related Documentation

- [Compliance Templates](../../../deploy/compliance/README.md) - Ready-to-deploy YAML templates
- [What to Monitor](../what-to-monitor.md) - Path recommendations by workload type
- [Quick Start](../../QUICK_START.md) - Initial deployment guide
