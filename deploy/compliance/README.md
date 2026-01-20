# Panoptes Compliance Templates

Ready-to-deploy compliance monitoring configurations for Kubernetes.

## Prerequisites

- Panoptes operators installed (argus-operator, janus-operator)
- CRDs registered: `kubectl get crd arguswatchers.argus.como-technologies.io janusguards.janus.como-technologies.io`

## Pod Label Reference

Each compliance framework uses a specific label to identify which pods to monitor:

| Framework | Label | Value | Command |
|-----------|-------|-------|---------|
| Base Security | `security.panoptes.io/monitored` | `true` | `kubectl label pod NAME security.panoptes.io/monitored=true` |
| PCI-DSS | `pci-dss/scope` | `in-scope` | `kubectl label pod NAME pci-dss/scope=in-scope` |
| HIPAA | `hipaa/scope` | `ephi` | `kubectl label pod NAME hipaa/scope=ephi` |
| SOC 2 | `soc2/scope` | `in-scope` | `kubectl label pod NAME soc2/scope=in-scope` |
| CIS Kubernetes | `cis/scope` | `kubernetes-audit` | `kubectl label pod NAME cis/scope=kubernetes-audit` |
| NIST 800-53 | `nist-800-53/scope` | `moderate` | `kubectl label pod NAME nist-800-53/scope=moderate` |
| GDPR | `gdpr/scope` | `personal-data` | `kubectl label pod NAME gdpr/scope=personal-data` |

**Multi-framework compliance:**
```bash
# Apply multiple labels for workloads that need multiple frameworks
kubectl label pod myapp-pod \
  pci-dss/scope=in-scope \
  soc2/scope=in-scope \
  security.panoptes.io/monitored=true
```

## Available Templates

| Directory | Framework | Files |
|-----------|-----------|-------|
| `base-security/` | General security baseline | `template.yaml` (simple) + kustomize files |
| `pci-dss/` | PCI-DSS 3.2.1/4.0 | `template.yaml` (simple) + kustomize files |
| `hipaa/` | HIPAA Security Rule | `template.yaml` (simple) + kustomize files |
| `soc2/` | SOC 2 Type II | `template.yaml` (simple) + kustomize files |
| `cis-kubernetes/` | CIS Kubernetes v1.8 | `template.yaml` (simple) + kustomize files |
| `nist-800-53/` | NIST 800-53 (FISMA/FedRAMP) | `template.yaml` (simple) + kustomize files |
| `gdpr/` | GDPR (EU Data Protection) | `template.yaml` (simple) + kustomize files |

Each directory contains:
- `template.yaml` - Single-file template for quick testing (kubectl apply -f)
- `kustomization.yaml` + individual resource files - For production use with namespace customization

## Deployment Options

### Option 1: Helm (Recommended for Spectro Cloud)

See [Spectro Quick Start](../../docs/SPECTRO_QUICK_START.md) for Helm-based deployment with presets.

### Option 2: Simple kubectl apply

Best for quick testing:

```bash
kubectl apply -f pci-dss/template.yaml
```

### Option 3: Kustomize (Production)

Best for production with namespace customization:

```bash
kubectl apply -k pci-dss/
```

## Quick Start

### 1. Label your pods

See the [Pod Label Reference](#pod-label-reference) table above for the correct label for your framework.

### 2. Apply the template

```bash
# Simple apply (for testing)
kubectl apply -f pci-dss/template.yaml

# Or with kustomize (for production)
kubectl apply -k pci-dss/
```

### 3. Verify deployment

```bash
kubectl get arguswatchers,janusguards -l compliance=pci-dss
```

## Customization with Kustomize

Each template includes a `kustomization.yaml` for easy customization.

### Change namespace

```bash
cd pci-dss
kustomize edit set namespace my-namespace
kubectl apply -k .
```

### Add common labels

Create an overlay:

```yaml
# overlays/production/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - ../../pci-dss
namespace: production
commonLabels:
  environment: production
  team: security
```

Apply:

```bash
kubectl apply -k overlays/production/
```

## Template Details

### Base Security

Foundation monitoring for any workload:
- User account files (`/etc/passwd`, `/etc/shadow`, etc.)
- SSH configuration and keys
- System logs integrity
- PAM authentication modules
- Scheduled tasks (cron)
- Suspicious tool execution auditing

### PCI-DSS

Payment Card Industry compliance:
- **10.5.5**: Log file integrity monitoring
- **11.5**: Critical system file change detection
- **7.1**: Access control enforcement
- **10.2**: Audit trail logging

### HIPAA

Healthcare data protection:
- **164.312(b)**: Audit controls
- **164.312(c)(1)**: Data integrity
- **164.312(d)**: Authentication
- **164.308(a)(1)**: Security management

### SOC 2

Trust services criteria:
- **CC6.1-3**: Logical access security
- **CC7.1**: System operations
- **CC7.2-3**: Monitoring and incident detection

### CIS Kubernetes

Kubernetes security hardening:
- **1.1.x**: Control plane configuration
- **4.1-4.2**: Worker node and kubelet config
- **5.1.x**: Service account tokens
- **5.4.x**: Host path and runtime socket access

### NIST 800-53

Federal information security controls (FISMA/FedRAMP):
- **SI-7**: Software, firmware, and information integrity
- **SI-7(1)**: Integrity verification using cryptographic mechanisms
- **AU-2/AU-3**: Audit events and content of audit records
- **AU-6**: Audit review, analysis, and reporting
- **AC-6**: Least privilege access control

### GDPR

EU data protection regulation:
- **Article 32**: Security of processing (integrity and confidentiality)
- **Article 30**: Records of processing activities
- **Article 33**: Breach notification support (detection)
- **Article 5(1)(e)**: Storage limitation

## Enforcement Mode

By default, JanusGuard resources start in `enforcing: false` (audit-only mode).

To enable enforcement after validating behavior:

```bash
kubectl patch janusguard pci-dss-access -p '{"spec":{"enforcing":true}}'
```

**Note**: CIS Kubernetes template has `enforcing: true` by default for runtime socket protection.
