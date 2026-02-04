# Security Incident Detection Quickstart

This guide shows security engineers how to deploy Panoptes for real-world container attack detection and prevention in Kubernetes clusters.

## Threat Landscape

Modern container environments face sophisticated attack patterns:

- **Persistence**: Attackers modify cron jobs, systemd units, SSH configurations, or shell profiles to maintain access across container restarts
- **Container Breakout**: Direct access to Docker/containerd/CRI-O runtime sockets allows full host compromise
- **Privilege Escalation**: Creation of SUID binaries or capability abuse to gain root privileges
- **Supply Chain Attacks**: Replacement of trusted system binaries with backdoored versions
- **Credential Access**: Theft of /etc/shadow, SSH keys, or Kubernetes service account tokens
- **Staging**: Use of writable directories (/tmp, /dev/shm) to download and execute attack tools

Panoptes detects these patterns using kernel-level monitoring (inotify/fanotify) that cannot be bypassed by container processes.

## Quick Setup

Deploy Panoptes monitoring in three commands:

```bash
# 1. Apply base security monitoring template
kubectl apply -f deploy/compliance/base-security/template.yaml

# 2. Label pods to monitor (or use namespace labels for all pods)
kubectl label pod <pod-name> panoptes.como-technologies.io/monitored=true

# 3. Verify watchers and guards are active
kubectl get aw,jg -o wide
```

You should see ArgusWatcher (aw) and JanusGuard (jg) resources in Active state.

## Detection Scenarios

### 1. Persistence Detection (ArgusWatcher)

**Threat**: Attackers establish persistence by modifying system configuration files that execute code automatically.

**ArgusWatcher Configuration**:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: detect-persistence
  namespace: production
spec:
  # Monitor pods with security label
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"

  # Paths where persistence mechanisms live
  paths:
    # Cron-based persistence
    - path: /etc/crontab
      recursive: false
      events: [modify, create, delete]
      tags: ["persistence", "cron", "critical"]

    - path: /etc/cron.d
      recursive: true
      maxDepth: 2
      events: [modify, create, delete]
      tags: ["persistence", "cron", "critical"]

    - path: /var/spool/cron
      recursive: true
      maxDepth: 2
      events: [modify, create, delete]
      tags: ["persistence", "cron", "critical"]

    # Systemd-based persistence
    - path: /etc/systemd/system
      recursive: true
      maxDepth: 3
      events: [create, modify, delete]
      tags: ["persistence", "systemd", "high"]

    - path: /usr/lib/systemd/system
      recursive: true
      maxDepth: 3
      events: [create, modify, delete]
      tags: ["persistence", "systemd", "high"]

    # SSH-based persistence
    - path: /root/.ssh/authorized_keys
      recursive: false
      events: [modify, create]
      tags: ["persistence", "ssh", "critical"]

    - path: /home
      recursive: true
      maxDepth: 3
      # Match .ssh/authorized_keys in any user home dir
      includePatterns: ["**/.ssh/authorized_keys"]
      events: [modify, create]
      tags: ["persistence", "ssh", "critical"]

    # Shell profile persistence
    - path: /root/.bashrc
      recursive: false
      events: [modify, create]
      tags: ["persistence", "profile", "medium"]

    - path: /root/.bash_profile
      recursive: false
      events: [modify, create]
      tags: ["persistence", "profile", "medium"]

    - path: /etc/profile.d
      recursive: true
      maxDepth: 1
      events: [create, modify, delete]
      tags: ["persistence", "profile", "medium"]

  # Real-time event streaming
  streamEvents: true

  # Retention for forensics
  retentionDays: 30
```

**Simulate Attack**:

```bash
# Attempt to add malicious cron job
kubectl exec <pod-name> -- sh -c "echo '* * * * * /tmp/evil' >> /etc/crontab"

# Or create new cron job file
kubectl exec <pod-name> -- sh -c "echo '*/5 * * * * root /tmp/backdoor.sh' > /etc/cron.d/malicious"
```

**Verify Detection**:

```bash
# Check ArgusWatcher events
kubectl describe aw detect-persistence

# View event stream (if streaming configured)
kubectl logs -n panoptes-system deployment/argus-operator -f

# Expected: Event with tags=["persistence", "cron", "critical"]
# Event type: MODIFY or CREATE
# Path: /etc/crontab or /etc/cron.d/malicious
```

### 2. Container Breakout Prevention (JanusGuard)

**Threat**: Direct access to container runtime sockets (Docker, containerd, CRI-O) allows attackers to escape the container and control the host.

**JanusGuard Configuration**:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: prevent-runtime-breakout
  namespace: production
spec:
  # Monitor pods with security label
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"

  # ENFORCING MODE: Block access attempts
  enforcing: true

  # Runtime socket paths
  paths:
    # Docker socket
    - path: /var/run/docker.sock
      permissions: deny
      events: [open, access]
      tags: ["breakout", "runtime", "critical"]

    # containerd socket
    - path: /run/containerd/containerd.sock
      permissions: deny
      events: [open, access]
      tags: ["breakout", "runtime", "critical"]

    # CRI-O socket
    - path: /var/run/crio/crio.sock
      permissions: deny
      events: [open, access]
      tags: ["breakout", "runtime", "critical"]

    # Kubernetes admin config
    - path: /etc/kubernetes/admin.conf
      permissions: deny
      events: [open, access]
      tags: ["breakout", "k8s", "critical"]

    # Host /proc access (container escape)
    - path: /proc/sys/kernel
      permissions: deny
      events: [open, modify]
      tags: ["breakout", "kernel", "critical"]

  # Log all enforcement actions
  streamEvents: true
  retentionDays: 90
```

**Simulate Attack**:

```bash
# Attempt to access Docker socket
kubectl exec <pod-name> -- cat /var/run/docker.sock

# Expected result: Permission denied (blocked by JanusGuard)

# Attempt to list containers via containerd socket
kubectl exec <pod-name> -- ls -la /run/containerd/containerd.sock
```

**Verify Prevention**:

```bash
# Check JanusGuard enforcement events
kubectl describe jg prevent-runtime-breakout

# View denial events
kubectl logs -n panoptes-system deployment/janus-operator -f

# Expected: Event with action=DENIED
# Tags: ["breakout", "runtime", "critical"]
# The access attempt should fail with permission denied
```

### 3. Supply Chain Attack Detection (ArgusWatcher)

**Threat**: Attackers replace legitimate system binaries with backdoored versions to maintain stealthy access.

**ArgusWatcher Configuration**:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: detect-binary-tampering
  namespace: production
spec:
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"

  paths:
    # Critical system binaries
    - path: /usr/bin
      recursive: true
      maxDepth: 1
      events: [create, modify, delete, move]
      tags: ["supply-chain", "binaries", "critical"]
      # Exclude common package manager locks
      excludePatterns: ["**/*.lock", "**/.dpkg-*"]

    - path: /usr/sbin
      recursive: true
      maxDepth: 1
      events: [create, modify, delete, move]
      tags: ["supply-chain", "binaries", "critical"]

    - path: /bin
      recursive: false
      events: [create, modify, delete, move]
      tags: ["supply-chain", "binaries", "critical"]

    - path: /sbin
      recursive: false
      events: [create, modify, delete, move]
      tags: ["supply-chain", "binaries", "critical"]

    # Shared libraries
    - path: /usr/lib
      recursive: true
      maxDepth: 2
      includePatterns: ["**/*.so", "**/*.so.*"]
      events: [create, modify, delete]
      tags: ["supply-chain", "libraries", "high"]

    - path: /lib
      recursive: true
      maxDepth: 2
      includePatterns: ["**/*.so", "**/*.so.*"]
      events: [create, modify, delete]
      tags: ["supply-chain", "libraries", "high"]

    # Package manager databases (detect compromised updates)
    - path: /var/lib/dpkg
      recursive: true
      maxDepth: 2
      events: [modify]
      tags: ["supply-chain", "packages", "medium"]

    - path: /var/lib/rpm
      recursive: true
      maxDepth: 2
      events: [modify]
      tags: ["supply-chain", "packages", "medium"]

  streamEvents: true
  retentionDays: 90
```

**Simulate Attack**:

```bash
# Replace a system binary
kubectl exec <pod-name> -- sh -c "cp /bin/sh /usr/bin/curl"

# Create a suspicious new binary
kubectl exec <pod-name> -- sh -c "cp /bin/sh /usr/bin/evil"

# Modify a shared library
kubectl exec <pod-name> -- sh -c "echo 'malicious' >> /usr/lib/libc.so.6"
```

**Verify Detection**:

```bash
kubectl describe aw detect-binary-tampering

# Expected events:
# - MODIFY event for /usr/bin/curl (if overwritten)
# - CREATE event for /usr/bin/evil
# - MODIFY event for /usr/lib/libc.so.6
# All tagged with ["supply-chain", "binaries"/"libraries", "critical"/"high"]
```

### 4. Credential Access Monitoring (JanusGuard)

**Threat**: Attackers steal credentials from /etc/shadow, SSH keys, or Kubernetes service account tokens for lateral movement.

**JanusGuard Configuration**:

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: audit-credential-access
  namespace: production
spec:
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"

  # AUDIT MODE: Log access without blocking (for forensics)
  enforcing: false

  paths:
    # System password database
    - path: /etc/shadow
      permissions: audit
      events: [open, access]
      tags: ["credential-access", "shadow", "critical"]

    - path: /etc/gshadow
      permissions: audit
      events: [open, access]
      tags: ["credential-access", "shadow", "medium"]

    # SSH private keys
    - path: /root/.ssh
      permissions: audit
      recursive: true
      maxDepth: 2
      includePatterns: ["**/id_rsa", "**/id_ed25519", "**/id_ecdsa"]
      events: [open, access]
      tags: ["credential-access", "ssh", "critical"]

    - path: /home
      permissions: audit
      recursive: true
      maxDepth: 4
      includePatterns: ["**/.ssh/id_*"]
      excludePatterns: ["**/.ssh/*.pub"]  # Exclude public keys
      events: [open, access]
      tags: ["credential-access", "ssh", "critical"]

    # Kubernetes service account tokens
    - path: /var/run/secrets/kubernetes.io/serviceaccount/token
      permissions: audit
      events: [open, access]
      tags: ["credential-access", "k8s-token", "high"]

    # Cloud provider credentials
    - path: /root/.aws/credentials
      permissions: audit
      events: [open, access]
      tags: ["credential-access", "cloud", "critical"]

    - path: /root/.azure
      permissions: audit
      recursive: true
      maxDepth: 2
      events: [open, access]
      tags: ["credential-access", "cloud", "critical"]

    - path: /root/.config/gcloud
      permissions: audit
      recursive: true
      maxDepth: 2
      events: [open, access]
      tags: ["credential-access", "cloud", "critical"]

    # Docker/container registry credentials
    - path: /root/.docker/config.json
      permissions: audit
      events: [open, access]
      tags: ["credential-access", "registry", "high"]

  streamEvents: true
  retentionDays: 90
```

**Simulate Attack**:

```bash
# Attempt to read shadow file
kubectl exec <pod-name> -- cat /etc/shadow

# Attempt to steal SSH key
kubectl exec <pod-name> -- cat /root/.ssh/id_rsa

# Read Kubernetes service account token
kubectl exec <pod-name> -- cat /var/run/secrets/kubernetes.io/serviceaccount/token
```

**Verify Detection**:

```bash
kubectl describe jg audit-credential-access

# Expected: Events logged with action=AUDIT (not blocked)
# Tags: ["credential-access", "shadow"/"ssh"/"k8s-token", severity]
# Use for forensic analysis and alerting
```

### 5. Staging Area Monitoring (ArgusWatcher)

**Threat**: Attackers download tools to writable directories (/tmp, /dev/shm) before execution. Early detection prevents privilege escalation.

**ArgusWatcher Configuration**:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: detect-staging-activity
  namespace: production
spec:
  selector:
    matchLabels:
      panoptes.como-technologies.io/monitored: "true"

  paths:
    # Temporary directories (common staging areas)
    - path: /tmp
      recursive: true
      maxDepth: 3
      events: [create, modify]
      # Focus on executables and scripts
      includePatterns: ["**/*.sh", "**/*.elf", "**/*.bin", "**/.*"]
      tags: ["staging", "tmp", "medium"]

    - path: /var/tmp
      recursive: true
      maxDepth: 3
      events: [create, modify]
      includePatterns: ["**/*.sh", "**/*.elf", "**/*.bin", "**/.*"]
      tags: ["staging", "tmp", "medium"]

    # Shared memory (often used for in-memory payloads)
    - path: /dev/shm
      recursive: true
      maxDepth: 2
      events: [create, modify]
      tags: ["staging", "shm", "high"]

    # User home directories (secondary staging)
    - path: /root
      recursive: true
      maxDepth: 3
      # Detect hidden files/directories (common for malware)
      includePatterns: ["**/.*"]
      # Exclude legitimate hidden files
      excludePatterns: [
        "**/.bashrc",
        "**/.bash_profile",
        "**/.bash_history",
        "**/.ssh/known_hosts"
      ]
      events: [create, modify]
      tags: ["staging", "home", "medium"]

    # World-writable directories
    - path: /var/cache
      recursive: true
      maxDepth: 2
      events: [create, modify]
      includePatterns: ["**/*.sh", "**/*.bin"]
      tags: ["staging", "cache", "low"]

  streamEvents: true
  retentionDays: 30
```

**Simulate Attack**:

```bash
# Download and stage a backdoor script
kubectl exec <pod-name> -- sh -c "echo '#!/bin/sh' > /tmp/backdoor.sh"

# Create hidden staging directory
kubectl exec <pod-name> -- sh -c "mkdir /tmp/.hidden && echo 'payload' > /tmp/.hidden/exploit"

# Stage binary in shared memory
kubectl exec <pod-name> -- sh -c "cp /bin/sh /dev/shm/payload.bin"
```

**Verify Detection**:

```bash
kubectl describe aw detect-staging-activity

# Expected events:
# - CREATE event for /tmp/backdoor.sh
# - CREATE events for /tmp/.hidden/* (matches .* pattern)
# - CREATE event for /dev/shm/payload.bin
# Tags: ["staging", "tmp"/"shm", severity]
```

## Alerting Pipeline

Configure Prometheus to alert on critical security events:

```yaml
# deploy/monitoring/prometheus-alerts.yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: panoptes-security-alerts
  namespace: panoptes-system
spec:
  groups:
    - name: panoptes.security
      interval: 30s
      rules:
        # Critical persistence attempts
        - alert: PersistenceMechanismDetected
          expr: |
            rate(argus_events_total{tags=~".*persistence.*"}[5m]) > 0
          for: 1m
          labels:
            severity: critical
            category: persistence
          annotations:
            summary: "Persistence mechanism detected in {{ $labels.namespace }}/{{ $labels.pod }}"
            description: "ArgusWatcher detected {{ $value }} persistence-related file modifications"

        # Container breakout attempts
        - alert: ContainerBreakoutAttempt
          expr: |
            rate(janus_denials_total{tags=~".*breakout.*"}[5m]) > 0
          for: 1m
          labels:
            severity: critical
            category: breakout
          annotations:
            summary: "Container breakout attempt blocked in {{ $labels.namespace }}/{{ $labels.pod }}"
            description: "JanusGuard blocked {{ $value }} runtime socket access attempts"

        # Supply chain compromise
        - alert: BinaryTamperingDetected
          expr: |
            rate(argus_events_total{tags=~".*supply-chain.*",path=~"/usr/(bin|sbin)/.*"}[5m]) > 0
          for: 1m
          labels:
            severity: critical
            category: supply-chain
          annotations:
            summary: "System binary modification in {{ $labels.namespace }}/{{ $labels.pod }}"
            description: "Critical binary tampering detected: {{ $labels.path }}"

        # Credential theft
        - alert: CredentialAccessAttempt
          expr: |
            rate(janus_events_total{tags=~".*credential-access.*"}[5m]) > 2
          for: 2m
          labels:
            severity: high
            category: credential-theft
          annotations:
            summary: "Suspicious credential access in {{ $labels.namespace }}/{{ $labels.pod }}"
            description: "Multiple credential file access attempts detected"
```

Deploy alerts:

```bash
kubectl apply -f deploy/monitoring/prometheus-alerts.yaml
```

Configure alert routing in Alertmanager to send notifications to Slack, PagerDuty, or your SIEM.

## Response Playbook

When a Panoptes security alert fires:

### Step 1: Identify Scope

```bash
# Get full event details from alert labels
kubectl describe aw <watcher-name> | grep -A 20 "Events:"
kubectl describe jg <guard-name> | grep -A 20 "Events:"

# Identify affected resources
NAMESPACE=<from-alert>
POD=<from-alert>
NODE=<from-event-metadata>
```

### Step 2: Assess Severity

Check event tags and paths:

- **Critical**: `persistence`, `breakout`, `supply-chain` tags on production workloads
- **High**: `credential-access` with multiple access attempts
- **Medium**: `staging` activity in non-development environments
- **Low**: Expected activity (package updates, legitimate admin operations)

### Step 3: Investigate

```bash
# View pod logs for suspicious activity
kubectl logs -n $NAMESPACE $POD --tail=100

# Check recent commands (if audit logging enabled)
kubectl exec -n $NAMESPACE $POD -- cat /var/log/audit/audit.log | tail -50

# Inspect process tree
kubectl exec -n $NAMESPACE $POD -- ps auxf

# Check network connections
kubectl exec -n $NAMESPACE $POD -- netstat -tunlp
```

### Step 4: Contain

For confirmed incidents:

```bash
# Isolate pod (delete to trigger recreation from clean image)
kubectl delete pod -n $NAMESPACE $POD

# Or apply network policy to block egress
kubectl apply -f - <<EOF
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: isolate-$POD
  namespace: $NAMESPACE
spec:
  podSelector:
    matchLabels:
      # Match compromised pod labels
  policyTypes:
    - Egress
  egress: []  # Block all egress
EOF

# Cordon node if host compromise suspected
kubectl cordon $NODE
```

### Step 5: Remediate

```bash
# Rotate credentials if accessed
kubectl delete secret <affected-secret> -n $NAMESPACE
kubectl create secret generic <affected-secret> --from-literal=...

# Update image to known-good version
kubectl set image deployment/<name> <container>=<image>:<known-good-tag>

# Enable enforcement mode to prevent recurrence
kubectl patch jg <guard-name> --type=merge -p '{"spec":{"enforcing":true}}'
```

### Step 6: Post-Incident

- Review Panoptes event retention (default 30-90 days) for full timeline
- Update detection rules based on attacker techniques
- Enhance monitoring for similar attack vectors
- Document findings in incident report

## Deep Dive Resources

- **Threat Modeling**: `/home/brett/repos/panoptes/docs/security/threat-model.md` - Complete attack tree analysis
- **Attack Surface**: `/home/brett/repos/panoptes/docs/security/attack-surface-analysis.md` - Kernel-level security architecture
- **Enforcement Guide**: `/home/brett/repos/panoptes/docs/guides/enabling-enforcement.md` - Safely deploying blocking policies
- **Monitoring Strategy**: `/home/brett/repos/panoptes/docs/guides/what-to-monitor.md` - Comprehensive path selection guide
- **Base Security Template**: `/home/brett/repos/panoptes/deploy/compliance/base-security/template.yaml` - Production-ready baseline

## Next Steps

1. **Start in Audit Mode**: Deploy all JanusGuards with `enforcing: false` to establish baselines
2. **Tune for Your Environment**: Exclude legitimate activity using `excludePatterns`
3. **Enable Enforcement Gradually**: Start with high-confidence rules (runtime socket blocking)
4. **Integrate with SIEM**: Stream events to your security operations center
5. **Automate Response**: Use Kubernetes admission webhooks to auto-isolate compromised pods
