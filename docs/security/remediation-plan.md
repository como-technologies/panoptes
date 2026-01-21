# Security Remediation Plan

> **Status:** COMPLETE
> **Priority:** HIGH
> **Target:** All compliance templates in `deploy/compliance/`

This document tracks the remediation of security gaps identified in the attack surface analysis.

---

## Phase 1: Critical Gaps (Immediate)

### 1.1 Add Staging Area Monitoring

**Status:** [x] COMPLETE
**Priority:** P0 - Critical
**Effort:** Low

**Problem:** `/tmp`, `/var/tmp`, `/dev/shm` are not monitored in any template.

**Files Modified:**
- [x] `deploy/compliance/base-security/arguswatcher.yaml`
- [x] `deploy/compliance/base-security/template.yaml`
- [x] `deploy/compliance/pci-dss/arguswatcher.yaml`
- [x] `deploy/compliance/pci-dss/template.yaml`
- [x] `deploy/compliance/hipaa/arguswatcher.yaml`
- [x] `deploy/compliance/hipaa/template.yaml`
- [x] `deploy/compliance/soc2/arguswatcher.yaml`
- [x] `deploy/compliance/soc2/template.yaml`
- [x] `deploy/compliance/nist-800-53/arguswatcher.yaml`
- [x] `deploy/compliance/nist-800-53/template.yaml`
- [x] `deploy/compliance/gdpr/arguswatcher.yaml`
- [x] `deploy/compliance/gdpr/template.yaml`
- [x] `deploy/compliance/cis-kubernetes/template.yaml`
- [x] `deploy/compliance/cis-kubernetes/arguswatcher-worker-node.yaml`

**Configuration to Add:**
```yaml
# Temporary file staging detection
- paths:
    - /tmp
    - /var/tmp
    - /dev/shm
  events:
    - create
    - modify
    - delete
  recursive: true
  maxDepth: 3
  ignore:
    - "*.sock"
    - "*.pid"
    - "pulse-*"
    - "ssh-*"
  tags:
    category: staging-detection
    severity: high
    attack: "T1074"  # Data Staged
```

**Test:**
```bash
# Apply template
kubectl apply -f deploy/compliance/pci-dss/template.yaml

# Create test file in staging area
kubectl exec test-pod -- touch /tmp/test-staging

# Verify event appears in UI or logs
kubectl logs -n panoptes-system -l app=argusd | grep staging
```

---

### 1.2 Add Library Path Monitoring

**Status:** [x] COMPLETE
**Priority:** P0 - Critical
**Effort:** Low

**Problem:** Only NIST monitors `/usr/lib`, `/lib`, `/lib64`. Library injection undetected in 6/7 templates.

**Files Modified:**
- [x] `deploy/compliance/base-security/arguswatcher.yaml`
- [x] `deploy/compliance/base-security/template.yaml`
- [x] `deploy/compliance/pci-dss/arguswatcher.yaml`
- [x] `deploy/compliance/pci-dss/template.yaml`
- [x] `deploy/compliance/hipaa/arguswatcher.yaml`
- [x] `deploy/compliance/hipaa/template.yaml`
- [x] `deploy/compliance/soc2/arguswatcher.yaml`
- [x] `deploy/compliance/soc2/template.yaml`
- [x] `deploy/compliance/gdpr/arguswatcher.yaml`
- [x] `deploy/compliance/gdpr/template.yaml`
- [x] `deploy/compliance/cis-kubernetes/template.yaml`
- [x] `deploy/compliance/cis-kubernetes/arguswatcher-worker-node.yaml`
- [x] `deploy/compliance/nist-800-53/arguswatcher.yaml` (added linker config files)
- [x] `deploy/compliance/nist-800-53/template.yaml` (added linker config files)

**Configuration to Add:**
```yaml
# Library injection detection
- paths:
    - /usr/lib
    - /lib
    - /lib64
    - /usr/lib64
  events:
    - create
    - modify
    - delete
  recursive: true
  maxDepth: 2
  ignore:
    - "*.pyc"
    - "__pycache__"
    - "*.cache"
  tags:
    category: library-integrity
    severity: critical
    attack: "T1574.006"  # LD_PRELOAD
```

**Test:**
```bash
# Apply template
kubectl apply -f deploy/compliance/pci-dss/template.yaml

# Create test library (will fail but generates event)
kubectl exec test-pod -- touch /usr/lib/test.so

# Verify event
kubectl logs -n panoptes-system -l app=argusd | grep library
```

---

### 1.3 Add Shell Config Monitoring

**Status:** [x] COMPLETE
**Priority:** P1 - High
**Effort:** Low

**Problem:** User shell configs (`.bashrc`, `.profile`) not monitored for backdoors.

**Files Modified:**
- [x] All template files and arguswatcher files (same as 1.1 and 1.2)

**Configuration to Add:**
```yaml
# Shell configuration backdoor detection
- paths:
    - /root/.bashrc
    - /root/.bash_profile
    - /root/.profile
    - /home/*/.bashrc
    - /home/*/.bash_profile
    - /home/*/.profile
    - /etc/profile
    - /etc/profile.d
    - /etc/bash.bashrc
  events:
    - modify
    - create
  recursive: false
  tags:
    category: shell-persistence
    severity: high
    attack: "T1546.004"  # Unix Shell Configuration
```

**Test:**
```bash
kubectl exec test-pod -- sh -c 'echo "# test" >> /root/.bashrc'
kubectl logs -n panoptes-system -l app=argusd | grep shell
```

---

### 1.4 Review autoAllowOwner Settings

**Status:** [x] COMPLETE (Option B implemented)
**Priority:** P1 - High
**Effort:** Low

**Problem:** `autoAllowOwner: true` on K8s secrets allows pod to read any mounted secret.

**Files Modified:**
- [x] `deploy/compliance/base-security/janusguard.yaml`
- [x] `deploy/compliance/base-security/template.yaml`
- [x] `deploy/compliance/pci-dss/janusguard.yaml`
- [x] `deploy/compliance/pci-dss/template.yaml`
- [x] `deploy/compliance/hipaa/janusguard.yaml`
- [x] `deploy/compliance/hipaa/template.yaml`
- [x] `deploy/compliance/soc2/janusguard.yaml`
- [x] `deploy/compliance/soc2/template.yaml`
- [x] `deploy/compliance/nist-800-53/janusguard.yaml`
- [x] `deploy/compliance/nist-800-53/template.yaml`
- [x] `deploy/compliance/gdpr/janusguard.yaml`
- [x] `deploy/compliance/gdpr/template.yaml`
- [x] `deploy/compliance/cis-kubernetes/janusguard.yaml`
- [x] `deploy/compliance/cis-kubernetes/template.yaml`

**Decision:** Option B selected - Keep `autoAllowOwner: true` but add security comments.

**Implementation:** Added security comments to all templates explaining:
- The permissive nature of autoAllowOwner
- How to configure stricter control
- Reference to attack surface analysis documentation

**Current (with security note):**
```yaml
# SECURITY NOTE: autoAllowOwner permits the pod to read any mounted secret.
# For stricter control, set autoAllowOwner: false and configure explicit
# RBAC policies. See docs/security/attack-surface-analysis.md section 3.2
- deny:
    - /var/run/secrets/kubernetes.io
  autoAllowOwner: true  # Permissive - see security note above
  audit: true
```

---

## Phase 2: Enforcement Mode (High Priority)

### 2.1 Document Enforcement Tradeoffs

**Status:** [x] COMPLETE
**Priority:** P1 - High
**Effort:** Medium

**Problem:** 6/7 templates use `enforcing: false` - violations logged but allowed.

**Task:** Add documentation explaining:
- [x] When to enable enforcement
- [x] Impact on application availability
- [x] Gradual rollout strategy (audit → enforce)
- [x] Rollback procedures

**File Created:**
- [x] `docs/guides/enabling-enforcement.md`

**Documentation includes:**
- Audit vs Enforcement mode comparison
- 4-week gradual rollout strategy
- Allowlist configuration examples
- Rollback procedures (immediate, specific rules, full)
- Monitoring and alerting guidance
- Troubleshooting section
- Template-specific guidance

---

### 2.2 Create Strict Template Variants

**Status:** [x] COMPLETE
**Priority:** P2 - Medium
**Effort:** Medium

**Task:** Create `-strict` variants with enforcement enabled.

**Implementation:** Created kustomize overlays that patch base templates with `enforcing: true`:
- [x] `deploy/compliance/base-security-strict/`
- [x] `deploy/compliance/pci-dss-strict/`
- [x] `deploy/compliance/hipaa-strict/`
- [x] `deploy/compliance/soc2-strict/`
- [x] `deploy/compliance/nist-800-53-strict/`
- [x] `deploy/compliance/gdpr-strict/`

**Usage:**
```bash
kubectl apply -k deploy/compliance/pci-dss-strict/
```

---

## Phase 3: Network Tool Blocking (Medium Priority)

### 3.1 Promote Network Tools from Audit to Deny

**Status:** [x] COMPLETE (Option C - Document Only)
**Priority:** P2 - Medium
**Effort:** Low

**Problem:** curl, wget, nc, nmap are audit-only - logged but allowed.

**Decision:** Option C selected - Keep audit-only, document limitation.

**Rationale (UNIX Philosophy):**
- Panoptes monitors **files** using inotify/fanotify
- Auditing `/usr/bin/curl` execution is a workaround, not true network monitoring
- True network monitoring requires eBPF network hooks (tcp_connect, socket syscalls)
- "Do one thing well" - network monitoring should be a separate tool (future Hermes daemon)

**Documentation Created:**
- [x] `docs/security/network-monitoring-roadmap.md` - Explains scope, limitations, and future Hermes architecture

**Current Behavior:**
Network tool execution is logged but allowed. This detects obvious attack patterns but is bypassable (compiled-in clients, raw sockets).

---

## Phase 4: Kernel Limit Hardening (Lower Priority)

### 4.1 Document inotify Tuning

**Status:** [x] COMPLETE
**Priority:** P3 - Low
**Effort:** Low

**Task:** Document kernel parameter tuning:
```bash
# Increase queue size (default: 16384)
echo 65536 > /proc/sys/fs/inotify/max_queued_events

# Increase watch limit (default: 8192)
echo 524288 > /proc/sys/fs/inotify/max_user_watches
```

**Files Created:**
- [x] `docs/operations/kernel-tuning.md`
- [x] `docs/operations/README.md`

**Documentation includes:**
- inotify tuning parameters (max_queued_events, max_user_watches, max_user_instances)
- Security risk explanation (queue overflow attack)
- Persistent sysctl configuration
- Kubernetes DaemonSet configuration examples
- fanotify tuning guidance
- Memory considerations
- Performance benchmarking scripts
- Troubleshooting section
- Complete node-level tuning script

---

### 4.2 Add Queue Overflow Alerting

**Status:** [x] COMPLETE
**Priority:** P2 - Medium
**Effort:** Medium

**Task:** Detect `IN_Q_OVERFLOW` / `FAN_Q_OVERFLOW` and generate alert.

**Files Created/Modified:**
- [x] `deploy/monitoring/prometheus-alerts.yaml` - PrometheusRule CRD with comprehensive alerts
- [x] `deploy/monitoring/README.md` - Documentation for alert configuration
- [x] `proto/argus/v2/argus.proto` - Added `queue_overflows` field to `WatchMetrics`
- [x] `proto/janus/v2/janus.proto` - Added `queue_overflows` field to `GuardMetrics`

**Daemon Metrics (Already Implemented):**
- `daemons/argusd/src/metrics.rs` - `queue_overflows: AtomicU64` counter
- `daemons/argusd/src/notify.rs` - Calls `record_queue_overflow()` on IN_Q_OVERFLOW
- `daemons/janusd/src/metrics.rs` - `queue_overflows: AtomicU64` counter
- `daemons/janusd/src/guard.rs` - Calls `record_queue_overflow()` on FAN_Q_OVERFLOW

**Alerts Created:**
- `PanoptesInotifyQueueOverflow` - Critical alert on queue overflow
- `PanoptesInotifyWatchLimitApproaching` - Warning at >80% watch usage
- `PanoptesHighEventRate` - Warning at >1000 events/sec
- `PanoptesCriticalPathDenied` - Critical on denied access to critical paths
- `PanoptesHighDenyRate` - Warning at >10 denies/sec
- `PanoptesCredentialAccessAttempt` - Warning on credential file access
- `PanoptesContainerEscapeAttempt` - Critical on runtime socket access
- `PanoptesArgusdDown` / `PanoptesJanusdDown` - Daemon health
- `PanoptesAuditLogTampering` - Log file modification
- `PanoptesSystemBinaryModified` - System binary changes
- `PanoptesStagingAreaActivity` - Activity in /tmp, /dev/shm

---

## Verification Checklist

### Per-Template Verification

For each template, verify:

```bash
TEMPLATE=pci-dss  # Change for each template

# 1. Dry-run apply
kubectl apply --dry-run=server -f deploy/compliance/$TEMPLATE/template.yaml

# 2. Apply to test cluster
kubectl apply -f deploy/compliance/$TEMPLATE/template.yaml

# 3. Verify watches registered
kubectl get arguswatcher -l compliance=$TEMPLATE -o wide
kubectl get janusguard -l compliance=$TEMPLATE -o wide

# 4. Test staging area detection
kubectl exec test-pod -- touch /tmp/test-file
kubectl exec test-pod -- touch /dev/shm/test-file

# 5. Test library path detection
kubectl exec test-pod -- touch /usr/lib/test.so 2>/dev/null || true

# 6. Check events in UI
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
# Navigate to Events, filter by category
```

---

## Completion Tracking

| Task | Status | Tested | Notes |
|------|--------|--------|-------|
| 1.1 Staging area monitoring | [x] | [ ] | All templates updated with `/tmp`, `/var/tmp`, `/dev/shm` |
| 1.2 Library path monitoring | [x] | [ ] | All templates updated with `/usr/lib`, `/lib`, `/lib64`, linker configs |
| 1.3 Shell config monitoring | [x] | [ ] | All templates updated with `/etc/profile`, `.bashrc`, etc. |
| 1.4 autoAllowOwner review | [x] | [ ] | Option B: Added security comments to all templates |
| 2.1 Enforcement docs | [x] | [ ] | Created `docs/guides/enabling-enforcement.md` |
| 2.2 Strict template variants | [x] | [ ] | Created 6 kustomize overlays with `enforcing: true` |
| 3.1 Network tool blocking | [x] | [ ] | Option C: Documented in `docs/security/network-monitoring-roadmap.md` |
| 4.1 Kernel tuning docs | [x] | [ ] | Created `docs/operations/kernel-tuning.md` |
| 4.2 Queue overflow alerting | [x] | [ ] | Proto fields + daemon metrics + Prometheus alerts |

---

## Sign-Off

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Security Lead | | | |
| Platform Lead | | | |
| QA | | | |
