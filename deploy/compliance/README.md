# Panoptes Compliance Templates

Ready-to-deploy compliance monitoring configurations for Kubernetes.

## Prerequisites

- Panoptes operators installed (argus-operator, janus-operator)
- CRDs registered: `kubectl get crd arguswatchers.argus.como-technologies.io janusguards.janus.como-technologies.io`

## Pod Label Reference

Each compliance framework uses a specific label to identify which pods to monitor:

| Framework | Label | Value | Command |
|-----------|-------|-------|---------|
| Base Security | `panoptes.como-technologies.io/monitored` | `true` | `kubectl label pod NAME panoptes.como-technologies.io/monitored=true` |
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
  panoptes.como-technologies.io/monitored=true
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

JanusGuard supports two operational modes:

| Mode | `enforcing:` | Behavior | Use Case |
|------|--------------|----------|----------|
| **Audit** | `false` | Log violations but allow access | Initial deployment, validation, troubleshooting |
| **Strict** | `true` | Block violations and log | Production enforcement after validation |

### Default Behavior

By default, all templates (except CIS Kubernetes) deploy in **audit mode** (`enforcing: false`).
This allows you to:
1. See what would be blocked without impacting workloads
2. Tune allowlists based on observed behavior
3. Validate the configuration before enabling enforcement

**Exception**: CIS Kubernetes template has `enforcing: true` by default because runtime socket access (`/var/run/docker.sock`, `/run/containerd/containerd.sock`) is a critical security boundary.

---

## Strict Template Variants

Pre-configured overlays with enforcement enabled are available for each framework:

| Framework | Audit-Only | Strict (Blocking) |
|-----------|------------|-------------------|
| Base Security | `base-security/` | `base-security-strict/` |
| PCI-DSS | `pci-dss/` | `pci-dss-strict/` |
| HIPAA | `hipaa/` | `hipaa-strict/` |
| SOC 2 | `soc2/` | `soc2-strict/` |
| NIST 800-53 | `nist-800-53/` | `nist-800-53-strict/` |
| GDPR | `gdpr/` | `gdpr-strict/` |
| CIS Kubernetes | Already strict | N/A |

### When to Use Strict Variants

**Use strict variants when:**
- You have validated the configuration in audit mode
- You understand what will be blocked
- Your workloads have been tested with enforcement
- You need compliance evidence of active enforcement

**Do NOT use strict variants when:**
- First deploying to a new environment
- You haven't reviewed audit logs for false positives
- Workloads haven't been tested with the rules
- You're troubleshooting application issues

---

## Deployment Workflows

### Option A: Start with Audit, Migrate to Strict (Recommended)

This is the safest approach for production environments.

**Step 1: Deploy in audit mode**
```bash
# Deploy audit-only configuration
kubectl apply -k pci-dss/

# Verify deployment
kubectl get janusguard pci-dss-access -o jsonpath='{.spec.enforcing}'
# Output: false
```

**Step 2: Monitor for 1-2 weeks**
```bash
# Watch for would-be violations
kubectl logs -n panoptes-system -l app=janusd -f | grep -i "audit\|deny"

# Check the Panoptes UI for events
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

**Step 3: Review and tune**
- Identify legitimate access patterns that would be blocked
- Add allowlist entries if needed
- Document expected blocked operations

**Step 4: Enable strict mode**
```bash
# Option A: Apply the strict overlay (replaces resources)
kubectl apply -k pci-dss-strict/

# Option B: Patch existing deployment
kubectl patch janusguard pci-dss-access -p '{"spec":{"enforcing":true}}'
```

**Step 5: Verify enforcement is active**
```bash
kubectl get janusguard pci-dss-access -o jsonpath='{.spec.enforcing}'
# Output: true
```

### Option B: Deploy Strict from Day One

For environments where you're confident in the configuration (e.g., copying from validated environment).

```bash
# Deploy with enforcement enabled
kubectl apply -k pci-dss-strict/

# Verify
kubectl get janusguard pci-dss-access -o jsonpath='{.spec.enforcing}'
# Output: true
```

**Warning**: Blocked operations will fail immediately. Ensure workloads are tested.

---

## Migrating Existing Deployments to Strict Mode

If you already have audit-only templates deployed and want to enable enforcement:

### Method 1: Apply Strict Overlay (Recommended)

The strict overlays are kustomize patches that override the base configuration:

```bash
# This replaces the existing JanusGuard with enforcing: true
kubectl apply -k pci-dss-strict/
```

**How it works**: The `-strict` directories contain kustomization.yaml files that:
1. Reference the base template as a resource
2. Apply a strategic merge patch to set `enforcing: true`

### Method 2: Patch Existing Resource

If you want to enable enforcement without redeploying:

```bash
# Enable enforcement on a specific JanusGuard
kubectl patch janusguard pci-dss-access -p '{"spec":{"enforcing":true}}'

# Enable enforcement on all JanusGuards in a namespace
kubectl get janusguard -o name | xargs -I {} kubectl patch {} -p '{"spec":{"enforcing":true}}'
```

### Method 3: Edit Resource Directly

```bash
kubectl edit janusguard pci-dss-access
# Change: enforcing: false
# To:     enforcing: true
```

---

## Rollback Procedures

If enforcement causes unexpected issues, you can quickly disable it:

### Immediate Rollback (Emergency)

```bash
# Disable enforcement immediately
kubectl patch janusguard pci-dss-access -p '{"spec":{"enforcing":false}}'

# Verify
kubectl get janusguard pci-dss-access -o jsonpath='{.spec.enforcing}'
# Output: false
```

### Rollback All JanusGuards

```bash
# Disable enforcement on all JanusGuards
kubectl get janusguard -A -o name | xargs -I {} kubectl patch {} -p '{"spec":{"enforcing":false}}'
```

### Revert to Audit-Only Template

```bash
# Redeploy the audit-only version
kubectl apply -k pci-dss/
```

### Pause Monitoring Entirely (Last Resort)

If you need to completely stop monitoring temporarily:

```bash
# Pause the JanusGuard (stops all monitoring)
kubectl patch janusguard pci-dss-access -p '{"spec":{"paused":true}}'

# Resume when ready
kubectl patch janusguard pci-dss-access -p '{"spec":{"paused":false}}'
```

---

## Verifying Enforcement Status

### Check Single Resource

```bash
kubectl get janusguard pci-dss-access -o jsonpath='{.spec.enforcing}'
```

### Check All JanusGuards

```bash
kubectl get janusguard -A -o custom-columns=\
'NAMESPACE:.metadata.namespace,NAME:.metadata.name,ENFORCING:.spec.enforcing,PAUSED:.spec.paused'
```

### Expected Output

```
NAMESPACE   NAME                  ENFORCING   PAUSED
default     pci-dss-access        true        false
default     base-security-access  false       false
```

---

## Strict Mode Checklist

Before enabling strict mode, verify:

- [ ] Deployed in audit mode for at least 1 week
- [ ] Reviewed all audit logs for false positives
- [ ] Added necessary allowlist entries
- [ ] Tested workloads with enforcement in staging
- [ ] Documented expected blocked operations
- [ ] Have rollback procedure ready
- [ ] Monitoring/alerting configured for denied events
- [ ] Stakeholders notified of enforcement timeline
