# Compliance Monitoring

> **Time:** 5 min (quick start) | 30+ min (deep dive)

Meet PCI-DSS, HIPAA, SOC 2, and other compliance requirements with file integrity monitoring and access auditing.

## Problem Statement

### The Challenge

Compliance frameworks like PCI-DSS require continuous monitoring of critical system files and audit logging of access events. Manual audits are time-consuming, and traditional FIM tools don't integrate well with Kubernetes.

### Who Needs This

- **Security/Compliance teams** preparing for PCI-DSS, HIPAA, SOC 2, or NIST audits
- **Platform engineers** implementing compliance-as-code
- **DevSecOps** teams adding compliance monitoring to CI/CD pipelines

### Compliance Context

This guide focuses on **PCI-DSS 10.5.5** (log file integrity monitoring) as the flagship example. The same patterns apply to:

| Framework | Key Requirements |
|-----------|-----------------|
| PCI-DSS | 10.5.5 (log integrity), 11.5 (change detection), 7.1 (access control) |
| HIPAA | 164.312(b) (audit controls), 164.312(c)(1) (data integrity) |
| SOC 2 | CC6.1 (access security), CC7.2 (system monitoring) |
| NIST 800-53 | SI-7 (software integrity), AU-2 (audit events) |

---

## Quick Start (5 Minutes)

### Step 1: Label Your Pods (30 seconds)

```bash
# Label pods that process payment card data
kubectl label pods -l app=payment-service pci-dss/scope=in-scope
```

### Step 2: Apply the PCI-DSS Template (30 seconds)

```bash
kubectl apply -f https://raw.githubusercontent.com/Como-Technologies/panoptes/main/deploy/compliance/pci-dss/template.yaml
```

Or apply locally:

```bash
kubectl apply -f deploy/compliance/pci-dss/template.yaml
```

### Step 3: Verify It's Working (2 minutes)

```bash
# Check ArgusWatcher status
kubectl get arguswatchers -l compliance=pci-dss

# Check JanusGuard status
kubectl get janusguards -l compliance=pci-dss

# Verify daemons are watching the pods
kubectl describe arguswatcher pci-dss-fim | grep -A5 "Status:"
```

### Step 4: View in Dashboard (1 minute)

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

Navigate to:
- **Compliance page**: See PCI-DSS score and check status
- **Events page**: Filter by `compliance: pci-dss` tag

---

## What Success Looks Like

### Expected Events

After applying the template, you'll see events like:

| Event Type | Path | Tag | Meaning |
|------------|------|-----|---------|
| `modify` | `/var/log/messages` | `requirement: 10.5.5` | Log file modified (normal) |
| `delete` | `/var/log/auth.log` | `requirement: 10.5.5`, `severity: critical` | Log deleted (investigate!) |
| `modify` | `/etc/passwd` | `requirement: 11.5` | User account changed |
| `access` | `/etc/shadow` | `requirement: 7.1` | Shadow file accessed (audit) |

### Dashboard State

- **Compliance page**: PCI-DSS framework shows green checkmarks for monitored requirements
- **Score**: Increases as monitoring coverage improves
- **Events page**: Events tagged with `compliance: pci-dss` and specific requirement numbers

---

## Deep Dive

### Understanding the PCI-DSS Template

The template creates two resources:

#### ArgusWatcher (File Integrity Monitoring)

Monitors for file changes that could indicate tampering:

```yaml
spec:
  subjects:
    # Requirement 10.5.5: Log file integrity
    - paths:
        - /var/log
      events:
        - modify
        - delete
      recursive: true
      tags:
        requirement: "10.5.5"
        severity: high

    # Requirement 11.5: Critical system files
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/sudoers
      events:
        - modify
        - delete
        - attrib
      tags:
        requirement: "11.5"
        severity: critical
```

#### JanusGuard (Access Control)

Audits (and optionally blocks) file access:

```yaml
spec:
  subjects:
    # Requirement 7.1: Access control
    - deny:
        - /etc/shadow
      allow:
        - /etc/passwd
      events:
        - access
        - open
      audit: true
      tags:
        requirement: "7.1"
        severity: critical

  enforcing: false  # Start in audit mode
```

### Multi-Framework Compliance

Apply multiple frameworks to the same workloads:

```bash
# Label for multiple frameworks
kubectl label pods -l app=healthcare-payment \
  pci-dss/scope=in-scope \
  hipaa/scope=ephi \
  soc2/scope=in-scope

# Apply all relevant templates
kubectl apply -f deploy/compliance/pci-dss/template.yaml
kubectl apply -f deploy/compliance/hipaa/template.yaml
kubectl apply -f deploy/compliance/soc2/template.yaml
```

### Enabling Enforcement Mode

After validating in audit mode (no false positives), enable enforcement:

```bash
# Enable blocking for JanusGuard
kubectl patch janusguard pci-dss-access -p '{"spec":{"enforcing":true}}'
```

**Warning:** Test thoroughly in audit mode first. Enforcement mode will block access that violates policies.

### Alerting Configuration

Create Prometheus alerts for compliance events:

```yaml
groups:
  - name: panoptes-pci-dss
    rules:
      - alert: PCIDSSLogFileTampered
        expr: increase(panoptes_argus_events_total{tags_requirement="10.5.5", event_type="delete"}[5m]) > 0
        for: 1m
        labels:
          severity: critical
          compliance: pci-dss
        annotations:
          summary: "Log file deleted - PCI-DSS 10.5.5 violation"
          description: "A log file was deleted on {{ $labels.node }}. This may indicate log tampering."

      - alert: PCIDSSShadowFileAccess
        expr: increase(panoptes_janus_events_total{tags_requirement="7.1", path="/etc/shadow"}[5m]) > 0
        for: 0m
        labels:
          severity: warning
          compliance: pci-dss
        annotations:
          summary: "Shadow file accessed - audit event"
```

### Exporting Compliance Reports

From the dashboard:

1. Navigate to **Compliance** page
2. Click **JSON** or **CSV** export buttons
3. Use exported reports for audit documentation

Programmatic export:

```bash
# Get compliance data via API
curl http://localhost:3000/api/compliance/report?framework=pci-dss
```

### Operational Considerations

#### Event Volume

- **Low volume**: Log modifications, user account changes (expected: 10-100/day)
- **High volume**: Access events if auditing all file access (tune with `ignore` patterns)

#### False Positives

Common false positives and how to handle:

| Event | Cause | Solution |
|-------|-------|----------|
| `/var/log/*.gz` modifications | Log rotation | Add to `ignore` list |
| `/etc/passwd` changes | Normal user management | Review, don't ignore |
| Frequent `/var/log` access | Application logging | Tune event types |

#### Recommended Review Cadence

- **Critical events** (log deletion, shadow access): Immediate alerts
- **High events** (config changes): Daily review
- **Medium events** (access audits): Weekly summary

---

## Troubleshooting

### No Events Appearing

1. **Check pod labels:**
   ```bash
   kubectl get pods -l pci-dss/scope=in-scope
   ```

2. **Verify selector matches:**
   ```bash
   kubectl get arguswatcher pci-dss-fim -o yaml | grep -A5 "selector:"
   ```

3. **Check daemon logs:**
   ```bash
   kubectl logs -n panoptes-system -l app=argusd --tail=50
   ```

### Too Many Events

Add ignore patterns to reduce noise:

```yaml
spec:
  subjects:
    - paths:
        - /var/log
      ignore:
        - "*.gz"
        - "*.old"
        - "*.[0-9]"
        - "*.tmp"
```

### Compliance Score Not Updating

1. Refresh the dashboard
2. Check that watchers/guards are not paused:
   ```bash
   kubectl get aw,jg -o custom-columns=NAME:.metadata.name,PAUSED:.spec.paused
   ```

---

## Related Documentation

- [Compliance Templates](../../../deploy/compliance/README.md) - All framework templates
- [What to Monitor](../what-to-monitor.md) - Path recommendations
- [Audit Logging](audit-logging.md) - Detailed access logging
