# Compliance Onboarding Quickstart

**Target Audience:** Platform engineers who need audit-ready file integrity monitoring deployed in minutes, not weeks.

**Time to Compliance:** ~15 minutes from zero to audit-ready monitoring.

## What Auditors Actually Need

When an auditor asks "How do you monitor file integrity?", they want four artifacts:

1. **Proof monitoring is active**: CRD status showing `watchesReady: true` and pod counts
2. **Logs of changes**: Event streams showing file modifications, who changed what, when
3. **Alert evidence**: Prometheus metrics or logs proving you detect anomalies
4. **Retention proof**: Log storage configuration meeting framework requirements (13 months for PCI-DSS, 7 years for HIPAA)

Panoptes delivers all four out of the box.

## Quick Path (3 Commands)

Choose your compliance framework and run:

```bash
# 1. Apply the compliance template (example: PCI-DSS)
kubectl apply -f deploy/compliance/pci-dss/template.yaml

# 2. Label your in-scope pods
kubectl label pod <pod-name> pci-dss/scope=in-scope

# 3. Verify monitoring is active
kubectl get aw -o wide
```

**Expected Output:**
```
NAME              WATCHED-PODS   EVENTS-DETECTED   WATCHES-READY   AGE
pci-dss-watcher   12             1847              true            5m
```

**That's it.** You now have audit-ready file integrity monitoring running. The sections below explain framework-specific requirements and evidence collection.

## Framework-Specific Guides

### PCI-DSS 4.0

**Requirements Covered:**
- **10.3.4**: Log all changes to authentication and authorization mechanisms
- **10.7**: Retain audit trail history for at least 13 months
- **11.5.2**: Deploy file integrity monitoring to detect unauthorized modification of critical files

**What's Monitored:**
- `/etc/passwd`, `/etc/shadow`, `/etc/group` (user/auth files)
- `/var/log/audit/`, `/var/log/secure` (audit logs)
- `/usr/bin/`, `/usr/sbin/` (system binaries)
- `/etc/ssh/sshd_config`, `/etc/pam.d/` (access controls)

**Pod Label:** `pci-dss/scope: in-scope`

**Deployment:**
```bash
# Apply PCI-DSS template
kubectl apply -f deploy/compliance/pci-dss/template.yaml

# Label pods handling cardholder data
kubectl label pod payment-api-7d4f5c-xyz pci-dss/scope=in-scope
kubectl label pod db-primary-5b8c9d-abc pci-dss/scope=in-scope

# Production deployment with retention policy
kubectl apply -k deploy/compliance/pci-dss/
```

**Verify:**
```bash
kubectl get aw pci-dss-watcher -o yaml
```

Look for:
- `status.watchesReady: true`
- `status.watchedPods > 0`
- `status.eventsDetected` incrementing over time

### HIPAA

**Requirements Covered:**
- **164.312(a)(1)**: Access control (unique user identification)
- **164.312(b)**: Audit controls (hardware, software, procedural mechanisms to record and examine activity)
- **164.312(c)(1)**: Integrity controls (protect ePHI from improper alteration)

**What's Monitored:**
- `/etc/passwd`, `/etc/shadow`, `/etc/group` (auth files)
- `/var/log/audit/`, `/var/log/secure` (audit trails)
- `/etc/ssh/sshd_config` (remote access controls)
- `/etc/pam.d/` (authentication mechanisms)

**Pod Label:** `hipaa/scope: ephi`

**Deployment:**
```bash
# Apply HIPAA template
kubectl apply -f deploy/compliance/hipaa/template.yaml

# Label pods processing ePHI (electronic Protected Health Information)
kubectl label pod patient-records-api-xyz hipaa/scope=ephi
kubectl label pod fhir-server-abc hipaa/scope=ephi

# Production deployment
kubectl apply -k deploy/compliance/hipaa/
```

**Retention Requirement:** 7 years minimum (6 years from creation or last effective date, per 164.316(b)(2)(i)).

### SOC 2

**Requirements Covered:**
- **CC6.1**: Logical and physical access controls restrict access to authorized users
- **CC7.1**: System monitoring detects security events and anomalies
- **CC7.2**: Infrastructure, data, and software are reviewed for security vulnerabilities

**What's Monitored:**
- `/etc/passwd`, `/etc/shadow`, `/etc/sudoers` (access controls)
- Configuration files (`/etc/`, app configs)
- Critical binaries (`/usr/bin/`, `/usr/sbin/`)
- Anomaly indicators (unexpected file creation in system directories)

**Pod Label:** `soc2/scope: in-scope`

**Deployment:**
```bash
# Apply SOC 2 template
kubectl apply -f deploy/compliance/soc2/template.yaml

# Label all pods in your service organization's control environment
kubectl label pod api-gateway-xyz soc2/scope=in-scope
kubectl label pod customer-db-abc soc2/scope=in-scope

# Production deployment
kubectl apply -k deploy/compliance/soc2/
```

**Audit Period:** Typically 12 months for Type 2 reports.

### CIS Kubernetes Benchmark

**Requirements Covered:**
- **5.2.1-5.2.13**: Pod Security Standards (file permissions, ownership)
- **5.7**: General Security Practices (file integrity monitoring)

**What's Monitored:**
- `/etc/kubernetes/` (control plane configs)
- `/etc/systemd/system/kubelet.service.d/` (kubelet config)
- PKI certificates (`/etc/kubernetes/pki/`)
- Audit policy files

**Pod Label:** `cis/scope: kubernetes-audit`

**Deployment:**
```bash
# Apply CIS Kubernetes template
kubectl apply -f deploy/compliance/cis-kubernetes/template.yaml

# Label control plane and critical infrastructure pods
kubectl label pod etcd-controlplane-xyz cis/scope=kubernetes-audit
kubectl label pod kube-apiserver-controlplane-abc cis/scope=kubernetes-audit

# Production deployment
kubectl apply -k deploy/compliance/cis-kubernetes/
```

### NIST 800-53

**Requirements Covered:**
- **CM-3**: Configuration change control
- **CM-6**: Configuration settings
- **SI-7**: Software, firmware, and information integrity

**What's Monitored:**
- System configuration files (`/etc/`)
- Authentication mechanisms (`/etc/pam.d/`, `/etc/ssh/`)
- Audit infrastructure (`/var/log/audit/`)
- Critical executables (`/usr/bin/`, `/usr/sbin/`)

**Pod Label:** `nist-800-53/scope: moderate` (or `low`, `high` for different impact levels)

**Deployment:**
```bash
# Apply NIST 800-53 template
kubectl apply -f deploy/compliance/nist-800-53/template.yaml

# Label pods per impact level (FIPS 199)
kubectl label pod public-api-xyz nist-800-53/scope=moderate
kubectl label pod classified-system-abc nist-800-53/scope=high

# Production deployment
kubectl apply -k deploy/compliance/nist-800-53/
```

### GDPR

**Requirements Covered:**
- **Article 5(1)(f)**: Integrity and confidentiality (security of processing)
- **Article 32**: Security of processing (appropriate technical measures)

**What's Monitored:**
- Access control files (`/etc/passwd`, `/etc/shadow`, `/etc/group`)
- Encryption configs (TLS, at-rest encryption)
- Audit logs (`/var/log/audit/`)
- Data processing application configs

**Pod Label:** `gdpr/scope: personal-data`

**Deployment:**
```bash
# Apply GDPR template
kubectl apply -f deploy/compliance/gdpr/template.yaml

# Label pods processing personal data (Article 4(1))
kubectl label pod user-profile-service-xyz gdpr/scope=personal-data
kubectl label pod analytics-pipeline-abc gdpr/scope=personal-data

# Production deployment
kubectl apply -k deploy/compliance/gdpr/
```

**Data Retention:** No specific retention for integrity logs, but must align with your data processing records (Article 30).

## Producing Audit Evidence

Auditors need proof that monitoring is active and effective. Here's how to generate evidence:

### Evidence 1: Monitoring Is Active

```bash
# Export watcher status as JSON
kubectl get aw -o json > evidence-monitoring-active.json

# Check specific watcher details
kubectl get aw pci-dss-watcher -o yaml
```

**What to highlight in the export:**
- `status.watchesReady: true` - Monitoring is active
- `status.observablePods: 12` - Number of labeled pods discovered
- `status.watchedPods: 12` - Number of pods actively monitored
- `status.eventsDetected: 1847` - Total events captured (proof of operation)
- `status.conditions[0].status: "True"` - Health status

**Auditor Translation:** "This shows our FIM system is deployed, discovered 12 in-scope pods, and has detected 1,847 file events."

### Evidence 2: Continuous Monitoring (Prometheus)

```bash
# Query Prometheus metrics
curl http://argus-operator:8080/metrics | grep argus_watcher

# Key metrics:
# argus_watcher_events_total{watcher="pci-dss-watcher"} 1847
# argus_watcher_watched_pods{watcher="pci-dss-watcher"} 12
# argus_watcher_watches_ready{watcher="pci-dss-watcher"} 1
```

**Auditor Translation:** "Our monitoring platform exports metrics that prove continuous operation. The incrementing event counter shows active detection."

### Evidence 3: Event Log Sample

```bash
# Export recent events from daemon logs (if using centralized logging)
kubectl logs -l app.kubernetes.io/name=argusd --tail=100 > evidence-event-sample.log
```

**What to highlight:**
- Event timestamps (proves detection)
- File paths (proves scope coverage)
- Event types (CREATE, MODIFY, DELETE)
- Process metadata (who/what made the change)

### Evidence 4: Retention Proof

For PCI-DSS (13 months), HIPAA (7 years), etc., show your log retention configuration:

```yaml
# Example: Centralized logging config (Loki, Elasticsearch, CloudWatch, etc.)
apiVersion: v1
kind: ConfigMap
metadata:
  name: log-retention-policy
data:
  retention-days: "395"  # 13 months for PCI-DSS
  # OR retention-days: "2555"  # 7 years for HIPAA
```

**Auditor Translation:** "Our centralized logging platform retains FIM events for [X years/months] per [framework] requirements."

## Enabling Enforcement (Gradual Path)

The templates above deploy in **audit-only mode** (monitoring without blocking). To enable enforcement:

**Recommendation:** Run in audit-only mode for 1-2 weeks first. Review event logs to ensure no false positives before enabling enforcement.

**When ready:**

See [enabling-enforcement.md](./enabling-enforcement.md) for:
- Dry-run testing
- Incremental rollout strategies
- Emergency bypass procedures
- Incident response runbooks

**Quick enforcement enable (after testing):**
```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: pci-dss-enforcement
spec:
  podSelector:
    matchLabels:
      pci-dss/scope: in-scope
  enforcementMode: enforce  # audit (default) -> enforce
  policies:
    - denyWrite:
        paths: ["/etc/passwd", "/etc/shadow"]
        action: block  # log -> block
```

## Production Checklist

Before going to production or showing evidence to auditors:

- [ ] **Deploy compliance template**: `kubectl apply -f deploy/compliance/<framework>/template.yaml`
- [ ] **Label all in-scope pods**: Use framework-specific label (e.g., `pci-dss/scope: in-scope`)
- [ ] **Verify watchers are active**: `kubectl get aw -o wide` shows `WATCHES-READY: true`
- [ ] **Configure log retention**: Set centralized logging to meet framework requirements (13 months PCI, 7 years HIPAA)
- [ ] **Set up Prometheus alerting**: Alert on `argus_watcher_watches_ready == 0` (monitoring down)
- [ ] **Review enforcement guide**: Read [enabling-enforcement.md](./enabling-enforcement.md) before enabling blocking mode
- [ ] **Document evidence collection**: Create runbook for extracting audit artifacts (JSON exports, metrics, logs)
- [ ] **Test evidence extraction**: Run through the "Producing Audit Evidence" section to ensure you can generate artifacts on demand
- [ ] **Validate pod coverage**: Ensure all in-scope workloads are labeled and appear in `status.observablePods`

## Troubleshooting

**Problem:** `status.watchedPods: 0` (no pods being monitored)

**Cause:** Pod selector doesn't match any labeled pods.

**Fix:**
```bash
# Check if pods have the right label
kubectl get pods -l pci-dss/scope=in-scope

# If empty, add labels
kubectl label pod <pod-name> pci-dss/scope=in-scope
```

---

**Problem:** `status.watchesReady: false`

**Cause:** Daemon not reachable or inotify watches failing.

**Fix:**
```bash
# Check daemon logs
kubectl logs -l app.kubernetes.io/name=argusd

# Common issues:
# - inotify watch limit exceeded (increase fs.inotify.max_user_watches)
# - Daemon not running (check DaemonSet)
# - gRPC connection failed (check network policies)
```

---

**Problem:** Events not appearing in logs

**Cause:** No file changes happening, or event streaming not configured.

**Fix:**
```bash
# Trigger a test event
kubectl exec <pod-name> -- touch /etc/test-fim-event

# Check daemon received it
kubectl logs -l app.kubernetes.io/name=argusd --tail=50 | grep test-fim-event
```

## Deep Dive Resources

- [monitoring-by-compliance.md](./monitoring-by-compliance.md) - Full compliance requirement mappings
- [enabling-enforcement.md](./enabling-enforcement.md) - Production enforcement rollout guide
- [what-to-monitor.md](./what-to-monitor.md) - File path selection strategies
- [../security/threat-model.md](../security/threat-model.md) - Security architecture and attack surface

## Support

For compliance-specific questions:
- Review template YAML comments: `deploy/compliance/<framework>/template.yaml`
- Check operator logs: `kubectl logs -l control-plane=argus-operator`
- File issues: [GitHub Issues](https://github.com/como-technologies/panoptes/issues)

**Remember:** Compliance is a journey, not a destination. Start with audit-only mode, build confidence in your event logs, then gradually enable enforcement.
