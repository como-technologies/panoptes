# Advanced Hardening Guide

> **Prerequisite:** Complete all items in [remediation-plan.md](./remediation-plan.md) first.
> **Audience:** Security teams deploying Panoptes in high-security environments.

This guide covers advanced hardening techniques beyond the baseline compliance templates.

---

## Overview

After completing the remediation plan, these additional measures provide defense-in-depth:

| Category | Baseline | Advanced |
|----------|----------|----------|
| Path monitoring | Critical paths | + /proc, /sys, kernel modules |
| Enforcement | Audit-only | Full enforcement with allowlists |
| Process attribution | /proc lookup | eBPF-based (eliminates TOCTOU) |
| Event integrity | Log to stdout | Audit subsystem + remote syslog |
| Runtime | containerd/cri-o | + gVisor, Kata detection |

---

## 1. Extended Path Monitoring

### 1.1 Kernel Module Monitoring

Monitor kernel module loading for rootkit detection:

```yaml
# Add to ArgusWatcher subjects
- paths:
    - /lib/modules
    - /etc/modprobe.d
    - /etc/modules-load.d
  events:
    - create
    - modify
    - delete
  recursive: true
  maxDepth: 3
  tags:
    category: kernel-modules
    severity: critical
    attack: "T1547.006"  # Kernel Modules and Extensions
```

### 1.2 Boot Process Integrity

Monitor boot-related files:

```yaml
- paths:
    - /boot
    - /etc/grub.d
    - /etc/default/grub
  events:
    - all
  recursive: true
  maxDepth: 2
  tags:
    category: boot-integrity
    severity: critical
    attack: "T1542"  # Pre-OS Boot
```

### 1.3 Container Runtime Deep Monitoring

Beyond socket access, monitor runtime configurations:

```yaml
- paths:
    - /etc/containerd
    - /etc/docker
    - /etc/crio
    - /var/lib/containerd
    - /var/lib/docker
  events:
    - create
    - modify
    - delete
  recursive: true
  maxDepth: 3
  ignore:
    - "*.log"
    - "containerd.sock"
  tags:
    category: runtime-config
    severity: critical
```

### 1.4 Process Namespace Monitoring

Monitor namespace-related paths (requires host access):

```yaml
# Note: Requires privileged daemonset
- paths:
    - /proc/*/ns
    - /proc/*/root
  events:
    - access
  tags:
    category: namespace-access
    severity: high
    note: "Requires CAP_SYS_PTRACE"
```

---

## 2. Full Enforcement Strategy

### 2.1 Gradual Rollout Process

**Week 1: Audit Mode**
```yaml
spec:
  enforcing: false
  # Collect baseline of normal access patterns
```

**Week 2: Analysis**
- Review all events in dashboard
- Identify false positives
- Add legitimate processes to allowlist

**Week 3: Selective Enforcement**
```yaml
spec:
  subjects:
    # High-confidence rules: enforce
    - deny:
        - /etc/shadow
      enforcing: true  # Per-subject override

    # Lower-confidence: still audit
    - deny:
        - /usr/bin/curl
      enforcing: false
```

**Week 4: Full Enforcement**
```yaml
spec:
  enforcing: true
```

### 2.2 Allowlist Configuration

Create explicit allowlists for legitimate access:

```yaml
spec:
  subjects:
    # Shadow file protection with allowlist
    - deny:
        - /etc/shadow
        - /etc/gshadow
      allow:
        - /etc/passwd
        - /etc/group
      allowProcesses:  # Future feature
        - /usr/bin/login
        - /usr/bin/su
        - /usr/bin/sudo
        - /usr/sbin/sshd
      events:
        - access
        - open
      enforcing: true
      audit: true
```

### 2.3 Namespace Isolation

Apply different policies per namespace:

```yaml
# Strict policy for production
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: production-strict
  namespace: production
spec:
  selector:
    matchLabels:
      environment: production
  enforcing: true
  # ... strict rules
---
# Relaxed policy for development
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: development-relaxed
  namespace: development
spec:
  selector:
    matchLabels:
      environment: development
  enforcing: false
  # ... relaxed rules
```

---

## 3. eBPF Integration (Future)

### 3.1 Benefits Over Traditional Mode

| Feature | fanotify | eBPF |
|---------|----------|------|
| Process attribution | /proc lookup (TOCTOU) | Atomic in-kernel |
| Container awareness | /proc/{pid}/root | Native cgroup context |
| Performance | User-space callbacks | In-kernel filtering |
| Tamper resistance | Moderate | High |

### 3.2 Enabling eBPF Mode

When available (requires BPF LSM-enabled kernel):

```bash
# Check BPF LSM availability
grep bpf /sys/kernel/security/lsm

# Enable in daemon
JANUSD_MODE=ebpf janusd --node-name=$(hostname)
```

### 3.3 Kernel Requirements

```bash
# Required kernel config
CONFIG_BPF=y
CONFIG_BPF_SYSCALL=y
CONFIG_BPF_LSM=y
CONFIG_DEBUG_INFO_BTF=y

# LSM order must include bpf
# /etc/default/grub or kernel cmdline:
lsm=lockdown,capability,yama,apparmor,bpf
```

---

## 4. Audit Subsystem Integration

### 4.1 Correlation with Linux Audit

Correlate Panoptes events with kernel audit logs:

```bash
# Enable audit rules for monitored paths
auditctl -w /etc/shadow -p rwxa -k shadow_access
auditctl -w /etc/cron.d -p wa -k cron_changes

# Query correlated events
ausearch -k shadow_access --start today
```

### 4.2 Remote Syslog for Tamper Resistance

Configure daemon to send events to remote syslog:

```yaml
# In helm values or daemonset
observability:
  syslog:
    enabled: true
    server: "syslog.security.internal:514"
    protocol: tcp
    tls: true
    facility: authpriv
```

### 4.3 Event Signing (Future)

Sign events to detect tampering:

```yaml
observability:
  signing:
    enabled: true
    keyPath: /etc/panoptes/signing.key
    algorithm: ed25519
```

---

## 5. Network Segmentation

### 5.1 Daemon Network Policy

Restrict daemon network access:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: panoptes-daemon-egress
  namespace: panoptes-system
spec:
  podSelector:
    matchLabels:
      app: janusd
  policyTypes:
    - Egress
  egress:
    # Only allow gRPC to operator
    - to:
        - namespaceSelector:
            matchLabels:
              name: panoptes-system
      ports:
        - port: 50052
          protocol: TCP
    # DNS
    - to:
        - namespaceSelector: {}
      ports:
        - port: 53
          protocol: UDP
```

### 5.2 Event Collector Network Policy

Restrict UI/collector access:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: panoptes-eye-ingress
  namespace: panoptes-system
spec:
  podSelector:
    matchLabels:
      app: panoptes-eye
  policyTypes:
    - Ingress
  ingress:
    # Only from monitoring namespace
    - from:
        - namespaceSelector:
            matchLabels:
              name: monitoring
      ports:
        - port: 3000
```

---

## 6. Multi-Cluster Correlation

### 6.1 Centralized Event Collection

Deploy central collector for multi-cluster visibility:

```yaml
# Central cluster
apiVersion: v1
kind: Service
metadata:
  name: panoptes-central
spec:
  type: LoadBalancer
  ports:
    - port: 4317
      name: otlp-grpc
---
# Remote clusters - export to central
observability:
  opentelemetry:
    enabled: true
    endpoint: "panoptes-central.security.internal:4317"
    headers:
      x-cluster-name: "prod-us-east-1"
```

### 6.2 Cross-Cluster Attack Detection

Detect lateral movement across clusters:

```yaml
# Prometheus alert for cross-cluster correlation
groups:
  - name: cross-cluster-attacks
    rules:
      - alert: MultiClusterCredentialAccess
        expr: |
          count by (path) (
            increase(panoptes_janus_events_total{
              tags_category="credential-access",
              action="deny"
            }[5m]) > 0
          ) > 2
        annotations:
          summary: "Credential access attempts across multiple clusters"
```

---

## 7. Compliance Evidence Collection

### 7.1 Automated Report Generation

Schedule compliance reports:

```bash
#!/bin/bash
# /etc/cron.daily/panoptes-compliance-report

DATE=$(date +%Y-%m-%d)
REPORT_DIR=/var/lib/panoptes/reports

# Export events for compliance period
curl -s "http://localhost:3000/api/events?since=24h&format=json" \
  > $REPORT_DIR/events-$DATE.json

# Generate summary
jq '{
  date: "'$DATE'",
  total_events: length,
  by_severity: group_by(.tags.severity) | map({key: .[0].tags.severity, count: length}) | from_entries,
  by_category: group_by(.tags.category) | map({key: .[0].tags.category, count: length}) | from_entries,
  denied_events: [.[] | select(.action == "deny")] | length
}' $REPORT_DIR/events-$DATE.json > $REPORT_DIR/summary-$DATE.json

# Encrypt and archive
gpg --encrypt --recipient compliance@company.com \
  $REPORT_DIR/events-$DATE.json
```

### 7.2 Chain of Custody

Maintain event integrity for legal/audit purposes:

```yaml
# Event metadata for chain of custody
observability:
  chainOfCustody:
    enabled: true
    includeFields:
      - timestamp
      - nodeId
      - podId
      - eventHash
      - previousHash  # Blockchain-style linking
```

---

## 8. Incident Response Integration

### 8.1 Automated Containment

Trigger automated responses to critical events:

```yaml
# Future: Event-driven response
responses:
  - match:
      tags:
        severity: critical
        category: credential-access
    actions:
      - type: annotate-pod
        annotation: "panoptes.como-technologies.io/quarantine=true"
      - type: webhook
        url: "https://siem.internal/incident"
      - type: slack
        channel: "#security-alerts"
```

### 8.2 Forensic Data Preservation

Preserve evidence on detection:

```bash
# Triggered by critical event
POD=$1
NAMESPACE=$2

# Snapshot pod state
kubectl get pod $POD -n $NAMESPACE -o yaml > /evidence/$POD-spec.yaml

# Capture logs
kubectl logs $POD -n $NAMESPACE --all-containers > /evidence/$POD-logs.txt

# Export Panoptes events for pod
curl "http://localhost:3000/api/events?pod=$POD" > /evidence/$POD-events.json

# Create forensic image (if configured)
crictl checkpoint --export=/evidence/$POD-checkpoint.tar $CONTAINER_ID
```

---

## 9. Performance Tuning

### 9.1 High-Volume Environments

For environments with >10,000 events/minute:

```yaml
# Increase daemon buffers
daemon:
  eventBufferSize: 10000
  deduplicationWindow: 5s
  batchSize: 100

# Kernel tuning
sysctls:
  fs.inotify.max_queued_events: "131072"
  fs.inotify.max_user_watches: "1048576"
```

### 9.2 Selective Monitoring

Reduce noise with targeted monitoring:

```yaml
# Instead of monitoring all of /var/log
- paths:
    - /var/log/auth.log
    - /var/log/secure
    - /var/log/audit/audit.log
  events:
    - modify
    - delete
  # Skip access events for performance
```

---

## 10. Testing and Validation

### 10.1 Red Team Scenarios

Test detection capabilities:

```bash
# Scenario 1: Persistence via cron
echo "* * * * * /tmp/beacon" > /etc/cron.d/test-persist
# Expected: ArgusWatcher create event, severity=high

# Scenario 2: Credential access
cat /etc/shadow
# Expected: JanusGuard deny event (if enforcing)

# Scenario 3: Container escape attempt
curl --unix-socket /var/run/docker.sock http://localhost/containers/json
# Expected: JanusGuard deny event (CIS-K8s enforcing)

# Scenario 4: Library injection
echo "/tmp/evil.so" >> /etc/ld.so.preload
# Expected: ArgusWatcher modify event on ld.so.preload
```

### 10.2 Chaos Engineering

Test resilience:

```bash
# Test queue overflow handling
stress-ng --io 16 --timeout 60s &
for i in $(seq 1 50000); do touch /tmp/chaos$i; rm /tmp/chaos$i; done

# Verify no event loss or daemon crash
kubectl logs -n panoptes-system -l app=argusd | grep -i overflow
```

---

## Appendix: Hardening Checklist

### Pre-Production

- [ ] Complete all remediation plan items
- [ ] Enable enforcement mode (or document exception)
- [ ] Configure remote syslog
- [ ] Deploy network policies
- [ ] Test all detection scenarios
- [ ] Document allowlisted processes

### Production

- [ ] Monitor queue overflow metrics
- [ ] Review events daily (first 2 weeks)
- [ ] Tune ignore patterns based on noise
- [ ] Configure compliance reporting
- [ ] Establish incident response procedures

### Ongoing

- [ ] Monthly: Review and update allowlists
- [ ] Quarterly: Red team testing
- [ ] Annually: Full security assessment
