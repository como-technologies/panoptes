# Security Incident Detection

> **Time:** 5 min (quick start) | 30+ min (deep dive)

Detect persistence mechanisms, privilege escalation, credential theft, and other security incidents.

## Problem Statement

### The Challenge

Attackers who gain initial access to a container often try to:
- **Persist** - Survive container restarts (cron jobs, startup scripts)
- **Escalate privileges** - Modify sudoers, add users, change permissions
- **Steal credentials** - Access shadow files, SSH keys, secrets
- **Move laterally** - Use network tools to pivot to other systems

Traditional security tools miss container-level threats because they focus on the host.

### Who Needs This

- **Security teams** monitoring for compromise indicators
- **SOC analysts** investigating container-based attacks
- **Incident responders** performing forensic analysis
- **Red teams** validating detection capabilities

### MITRE ATT&CK Coverage

| Technique ID | Name | Detection Method |
|--------------|------|------------------|
| T1053.003 | Cron | Monitor `/var/spool/cron`, `/etc/cron.d` |
| T1136.001 | Create Account | Monitor `/etc/passwd`, `/etc/shadow` |
| T1098 | Account Manipulation | Monitor `/etc/sudoers`, `/etc/group` |
| T1552.001 | Credentials in Files | Monitor `/etc/shadow`, `.ssh/` |
| T1059 | Command Interpreter | Audit execution of shells, interpreters |

---

## Quick Start (5 Minutes)

### Step 1: Label Your Pods (30 seconds)

```bash
# Label all pods for security monitoring
kubectl label pods --all security.panoptes.io/monitored=true
```

### Step 2: Apply the Base Security Template (30 seconds)

```bash
kubectl apply -f https://raw.githubusercontent.com/Como-Technologies/panoptes/main/deploy/compliance/base-security/template.yaml
```

Or apply locally:

```bash
kubectl apply -f deploy/compliance/base-security/template.yaml
```

### Step 3: Verify It's Working (2 minutes)

```bash
# Check both ArgusWatcher and JanusGuard
kubectl get arguswatchers,janusguards -l panoptes.io/template

# Verify they're watching pods
kubectl describe arguswatcher base-security-fim | grep -A5 "Watched Pods"
```

### Step 4: View in Dashboard (1 minute)

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

Navigate to **Events** page and filter by `severity: critical`

---

## What Success Looks Like

### Critical Security Events

These events should trigger immediate investigation:

| Event Type | Path | Severity | Threat |
|------------|------|----------|--------|
| `modify` | `/etc/passwd` | Critical | Account creation/modification |
| `modify` | `/etc/shadow` | Critical | Password change |
| `modify` | `/etc/sudoers` | Critical | Privilege escalation |
| `create` | `/etc/cron.d/*` | Critical | Persistence mechanism |
| `access` | `/etc/shadow` | Critical | Credential theft attempt |
| `modify` | `/root/.ssh/authorized_keys` | Critical | SSH backdoor |
| `modify` | `/usr/bin/*` | Critical | Binary tampering |

### Dashboard State

- **Events page**: Critical events highlighted in red
- **Severity filter**: Show only `critical` and `high` events
- **Process info**: See which process made the change (if available)

---

## Deep Dive

### Persistence Detection

Monitor all common persistence locations:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: persistence-detector
spec:
  selector:
    matchLabels:
      security.panoptes.io/monitored: "true"
  subjects:
    # Cron-based persistence
    - paths:
        - /var/spool/cron
        - /etc/cron.d
        - /etc/cron.daily
        - /etc/cron.hourly
        - /etc/crontab
      events: [create, modify, delete]
      recursive: true
      tags:
        attack: persistence
        technique: T1053.003
        severity: critical

    # Init script persistence
    - paths:
        - /etc/init.d
        - /etc/rc.local
        - /etc/systemd/system
      events: [create, modify]
      recursive: true
      tags:
        attack: persistence
        technique: T1037
        severity: critical

    # Shell profile persistence
    - paths:
        - /etc/profile
        - /etc/profile.d
        - /etc/bash.bashrc
        - /root/.bashrc
        - /root/.profile
      events: [modify, create]
      tags:
        attack: persistence
        technique: T1546.004
        severity: high
```

### Privilege Escalation Detection

```yaml
subjects:
  # User account manipulation
  - paths:
      - /etc/passwd
      - /etc/shadow
      - /etc/group
      - /etc/gshadow
    events: [modify, delete]
    tags:
      attack: privilege-escalation
      technique: T1136.001
      severity: critical

  # Sudo configuration
  - paths:
      - /etc/sudoers
      - /etc/sudoers.d
    events: [modify, create, delete]
    recursive: true
    tags:
      attack: privilege-escalation
      technique: T1548.003
      severity: critical

  # SUID/SGID binary creation
  - paths:
      - /usr/bin
      - /usr/sbin
      - /usr/local/bin
    events: [create, attrib]
    tags:
      attack: privilege-escalation
      technique: T1548.001
      severity: critical
```

### Credential Theft Prevention

Use JanusGuard to audit (or block) credential access:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: credential-protection
spec:
  selector:
    matchLabels:
      security.panoptes.io/monitored: "true"
  subjects:
    # Block shadow file access
    - deny:
        - /etc/shadow
        - /etc/gshadow
      allow:
        - /etc/passwd
        - /etc/group
      events: [access, open]
      audit: true
      tags:
        attack: credential-access
        technique: T1552.001
        severity: critical

    # Audit SSH key access
    - deny:
        - /root/.ssh/id_*
        - /root/.ssh/authorized_keys
        - /home/*/.ssh/id_*
      events: [access, open]
      audit: true
      tags:
        attack: credential-access
        technique: T1552.004
        severity: critical

    # Audit Kubernetes secrets
    - deny:
        - /var/run/secrets/kubernetes.io
      events: [access, open]
      autoAllowOwner: true  # Allow the owning process
      audit: true
      tags:
        attack: credential-access
        severity: high

  enforcing: false  # Start in audit mode
  logFormat: json
```

### Suspicious Tool Execution Auditing

Monitor execution of tools commonly used by attackers:

```yaml
subjects:
  # Network reconnaissance tools
  - deny:
      - /usr/bin/nmap
      - /usr/bin/netcat
      - /usr/bin/nc
      - /usr/bin/tcpdump
      - /usr/bin/wireshark
    events: [execute]
    audit: true
    tags:
      category: recon-tools
      severity: high

  # Data exfiltration tools
  - deny:
      - /usr/bin/curl
      - /usr/bin/wget
      - /usr/bin/scp
      - /usr/bin/rsync
      - /usr/bin/ftp
    events: [execute]
    audit: true  # Don't block, just log
    tags:
      category: exfil-tools
      severity: medium

  # Reverse shell indicators
  - deny:
      - /bin/bash
      - /bin/sh
      - /usr/bin/python*
      - /usr/bin/perl
      - /usr/bin/ruby
    events: [execute]
    audit: true
    tags:
      category: interpreters
      severity: low
```

### Enabling Enforcement Mode

After validating detection (no false positives), enable blocking:

```bash
# Enable enforcement
kubectl patch janusguard credential-protection -p '{"spec":{"enforcing":true}}'

# Verify enforcement is active
kubectl get janusguard credential-protection -o jsonpath='{.spec.enforcing}'
```

**Warning:** Test thoroughly first. Enforcement will block legitimate access that matches deny rules.

### Alerting Configuration

```yaml
groups:
  - name: panoptes-security-incidents
    rules:
      - alert: PersistenceMechanismDetected
        expr: increase(panoptes_argus_events_total{tags_attack="persistence"}[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Persistence mechanism detected"
          description: "{{ $labels.event_type }} on {{ $labels.path }} in pod {{ $labels.pod }}"

      - alert: CredentialAccessAttempt
        expr: increase(panoptes_janus_events_total{tags_attack="credential-access"}[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Credential access attempt detected"
          description: "Access to {{ $labels.path }} from {{ $labels.pod }}"

      - alert: PrivilegeEscalationAttempt
        expr: increase(panoptes_argus_events_total{tags_attack="privilege-escalation"}[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Privilege escalation attempt detected"
```

### Incident Response Workflow

When a critical event is detected:

1. **Triage** - View event details in dashboard
2. **Contain** - Enable enforcement mode if not already active
3. **Investigate** - Check pod logs, process history
4. **Remediate** - Delete compromised pod, rotate credentials
5. **Document** - Export events for incident report

```bash
# Quick containment - pause the pod's network
kubectl annotate pod $COMPROMISED_POD kubernetes.io/ingress-bandwidth=1

# Get all events for the pod
kubectl logs -n panoptes-system -l app=argusd | grep $COMPROMISED_POD

# Export for forensics
curl "http://localhost:3000/api/events?pod=$COMPROMISED_POD" > incident-events.json
```

### Operational Considerations

#### False Positive Sources

| Event | Common False Positive | Solution |
|-------|----------------------|----------|
| `/etc/passwd` access | Container init reading users | Filter by process name |
| `/var/spool/cron` | Legitimate cron jobs | Allowlist known jobs |
| Network tool execution | Health checks | Add to ignore or allowlist |

#### Event Volume

- **Critical events**: Should be near-zero in production (immediate investigation)
- **High events**: 10-50/day typical (daily review)
- **Medium events**: Higher volume (weekly summary)

---

## Troubleshooting

### Critical Events Not Triggering Alerts

1. **Check Prometheus scraping:**
   ```bash
   kubectl port-forward -n monitoring svc/prometheus 9090:9090
   # Query: panoptes_argus_events_total
   ```

2. **Verify alert rules loaded:**
   ```bash
   kubectl get prometheusrules -A
   ```

### Too Many False Positives

1. Add legitimate processes to allowlist
2. Use `autoAllowOwner: true` for JanusGuard
3. Tune ignore patterns for known-good paths

### Events Missing Process Information

Process attribution depends on the kernel and container runtime. Check:
```bash
kubectl logs -n panoptes-system -l app=argusd | grep "process"
```

---

## Related Documentation

- [Base Security Template](../../../deploy/compliance/base-security/template.yaml) - Complete security template
- [Audit Logging](audit-logging.md) - Comprehensive access logging
- [What to Monitor](../what-to-monitor.md) - Security-focused path recommendations
