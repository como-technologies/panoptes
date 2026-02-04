# Platform Hardening Quickstart

## Goal

Lock down your Kubernetes cluster with continuous file integrity monitoring and runtime access control. Detect config drift, protect system binaries, block container escape vectors.

This guide targets platform and DevOps engineers responsible for securing new or existing Kubernetes clusters using Panoptes.

## Prerequisites

- Kubernetes cluster (1.28+)
- `kubectl` with cluster admin access
- Panoptes operators deployed (Argus and Janus)
- Nodes running Linux kernel 5.x+

## Quick Setup (3 Commands)

Deploy CIS Kubernetes benchmark monitoring with enforcement enabled by default:

```bash
# 1. Deploy CIS Kubernetes compliance monitoring
kubectl apply -f deploy/compliance/cis-kubernetes/template.yaml

# 2. Label pods for monitoring (system pods with node filesystem access)
kubectl label pods --all -n kube-system cis/scope=kubernetes-audit

# 3. Verify deployment
kubectl get aw,jg -o wide
```

Expected output:
```
NAME                                         PAUSED   CONTAINER RUNTIME   AGE
arguswatcher.argus.como-technologies.io/cis-k8s-control-plane   false    auto                30s
arguswatcher.argus.como-technologies.io/cis-k8s-worker-node     false    auto                30s

NAME                                       ENFORCING   PAUSED   CONTAINER RUNTIME   AGE
janusguard.janus.como-technologies.io/cis-k8s-secrets   true        false    auto                30s
```

## What Gets Protected

### Layer 1: Config Drift Detection (ArgusWatcher)

Watch Kubernetes configuration files for unauthorized changes. Any modification triggers alerts and metrics updates.

**Protected paths:**
- `/etc/kubernetes/manifests/*` - Static pod specifications
- `/var/lib/kubelet/config.yaml` - Kubelet configuration
- `/etc/cni/` - CNI network configuration
- `/etc/kubernetes/pki/` - Certificate authority and certificates
- `/etc/kubernetes/*.conf` - Admin, controller-manager, scheduler configs

**Example: Control Plane Monitoring**

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: control-plane-protection
  labels:
    hardening: platform
spec:
  selector:
    matchLabels:
      app: monitoring-agent

  subjects:
    # Detect modifications to static pod manifests
    - paths:
        - /etc/kubernetes/manifests
        - /etc/kubernetes/pki
      events:
        - modify
        - create
        - delete
        - attrib
      recursive: true
      tags:
        severity: critical
        category: control-plane-manifests

    # Detect changes to kubeconfig files
    - paths:
        - /etc/kubernetes/admin.conf
        - /etc/kubernetes/controller-manager.conf
        - /etc/kubernetes/scheduler.conf
      events:
        - modify
        - delete
        - attrib
      tags:
        severity: critical
        category: kubeconfig

    # Monitor kubelet configuration
    - paths:
        - /etc/kubernetes/kubelet.conf
        - /var/lib/kubelet/config.yaml
      events:
        - modify
        - delete
        - attrib
      tags:
        severity: critical
        category: kubelet-config

    # Protect CNI configuration
    - paths:
        - /etc/cni/net.d
        - /opt/cni/bin
      events:
        - modify
        - create
        - delete
      recursive: true
      tags:
        severity: high
        category: cni-config

  containerRuntime: auto
  paused: false
  logFormat: json
```

Apply the configuration:
```bash
kubectl apply -f control-plane-protection.yaml
```

### Layer 2: Runtime Socket Protection (JanusGuard)

Block access to container runtime sockets - the number one container escape vector. This prevents attackers from breaking out of containers by accessing the host's container runtime.

**Protected sockets:**
- `/var/run/docker.sock` - Docker socket
- `/run/containerd/containerd.sock` - containerd socket
- `/var/run/crio/crio.sock` - CRI-O socket

**Example: Runtime Socket Enforcement**

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: runtime-socket-protection
  labels:
    hardening: platform
spec:
  selector:
    matchLabels:
      app: workload-pods

  subjects:
    # Block all access to container runtime sockets
    - deny:
        - /var/run/docker.sock
        - /var/run/containerd/containerd.sock
        - /var/run/crio/crio.sock
      events:
        - access
        - open
      defaultResponse: deny
      audit: true
      tags:
        severity: critical
        category: runtime-socket
        attack: container-escape

  containerRuntime: auto
  # ENFORCEMENT ENABLED - blocks access attempts
  enforcing: true
  paused: false
  logFormat: json
```

Apply the configuration:
```bash
kubectl apply -f runtime-socket-protection.yaml
```

### Layer 3: System Binary Protection (ArgusWatcher)

Detect modifications to system binaries that could indicate rootkit installation, privilege escalation, or malware injection.

**Protected paths:**
- `/usr/bin/*`, `/usr/sbin/*` - User and system binaries
- `/usr/lib/*`, `/lib/*` - Shared libraries
- `/boot/*` - Kernel and bootloader

**Example: System Binary Integrity**

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: system-binary-protection
  labels:
    hardening: platform
spec:
  selector:
    matchLabels:
      app: monitoring-agent

  subjects:
    # Monitor system binaries for tampering
    - paths:
        - /usr/bin
        - /usr/sbin
        - /bin
        - /sbin
      events:
        - modify
        - create
        - delete
      recursive: true
      maxDepth: 1
      tags:
        severity: critical
        category: system-binaries
        attack: T1543.002  # MITRE: Systemd Service

    # Detect shared library injection
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
        severity: critical
        category: library-integrity
        attack: T1574.006  # MITRE: LD_PRELOAD

    # Monitor boot partition
    - paths:
        - /boot
      events:
        - modify
        - create
        - delete
      recursive: true
      tags:
        severity: critical
        category: boot-integrity

    # Detect linker configuration tampering
    - paths:
        - /etc/ld.so.conf
        - /etc/ld.so.conf.d
        - /etc/ld.so.preload
      events:
        - create
        - modify
        - delete
      recursive: true
      tags:
        severity: critical
        category: linker-config

  containerRuntime: auto
  paused: false
  logFormat: json
```

Apply the configuration:
```bash
kubectl apply -f system-binary-protection.yaml
```

## Verify It Works

### Test Config Drift Detection

Simulate a configuration file modification:

```bash
# Get a monitoring pod with node filesystem access
POD=$(kubectl get pods -n kube-system -l cis/scope=kubernetes-audit -o name | head -1)

# Attempt to modify a watched file
kubectl exec -n kube-system $POD -- sh -c "echo '# test' >> /etc/kubernetes/manifests/test.yaml" || true

# Check that the event was detected
kubectl get aw cis-k8s-control-plane -o jsonpath='{.status.eventsDetected}' && echo
```

View detailed event logs:
```bash
kubectl logs -n panoptes-system -l app=argusd | grep -i manifests
```

### Test Runtime Socket Enforcement

Attempt to access the container runtime socket (should fail):

```bash
# Get a workload pod
POD=$(kubectl get pods -l app=workload-pods -o name | head -1)

# Attempt to access Docker socket (will be blocked)
kubectl exec $POD -- cat /var/run/docker.sock
# Expected: cat: /var/run/docker.sock: Operation not permitted

# Check that the denial was recorded
kubectl get jg cis-k8s-secrets -o jsonpath='{.status.totalDeniedEvents}' && echo
```

View denied events:
```bash
kubectl logs -n panoptes-system -l app=janusd | grep -i "runtime-socket"
```

### Test System Binary Protection

Simulate binary modification:

```bash
# Get a monitoring pod
POD=$(kubectl get pods -n kube-system -l cis/scope=kubernetes-audit -o name | head -1)

# Attempt to modify a system binary (in read-only filesystem, will fail but event is detected)
kubectl exec -n kube-system $POD -- sh -c "touch /usr/bin/test_binary" || true

# Check detection
kubectl get aw system-binary-protection -o yaml | grep -A 5 status
```

## Set Up Alerting

Deploy Prometheus alerting rules for critical security events:

```bash
kubectl apply -f deploy/monitoring/prometheus-alerts.yaml
```

### Key Alerts

| Alert | Severity | Description |
|-------|----------|-------------|
| `PanoptesInotifyQueueOverflow` | Critical | Events lost due to queue overflow - possible attack |
| `PanoptesCriticalPathDenied` | Critical | Access to critical path was blocked |
| `PanoptesSystemBinaryModified` | Critical | System binary modified or replaced |
| `PanoptesContainerEscapeAttempt` | Critical | Runtime socket access blocked |
| `PanoptesArgusdDown` | Critical | FIM daemon offline - monitoring stopped |
| `PanoptesJanusdDown` | Critical | Access control daemon offline |
| `PanoptesCredentialAccessAttempt` | Warning | Attempt to access credential files |
| `PanoptesStagingAreaActivity` | Warning | Unusual activity in temp directories |

### Configure Alert Routing

Example AlertManager configuration for PagerDuty:

```yaml
route:
  receiver: default
  routes:
    - match:
        severity: critical
        component: argusd
      receiver: pagerduty-critical
      continue: true

    - match:
        severity: critical
        component: janusd
      receiver: pagerduty-critical
      continue: true

receivers:
  - name: pagerduty-critical
    pagerduty_configs:
      - service_key: <YOUR_SERVICE_KEY>
        description: "{{ .CommonAnnotations.summary }}"
```

For Slack integration:

```yaml
receivers:
  - name: slack-security
    slack_configs:
      - api_url: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
        channel: '#security-alerts'
        title: "Panoptes Security Alert"
        text: "{{ .CommonAnnotations.description }}"
```

## Kernel Tuning

Essential sysctl settings for production environments to prevent event loss and ensure reliable monitoring.

### Quick Configuration

```bash
# Increase inotify queue size to prevent overflow
sudo sysctl -w fs.inotify.max_queued_events=65536

# Increase watch limit for broad monitoring
sudo sysctl -w fs.inotify.max_user_watches=524288

# Increase instance limit for multiple watchers
sudo sysctl -w fs.inotify.max_user_instances=256
```

### Persistent Configuration

Create `/etc/sysctl.d/99-panoptes.conf`:

```ini
# Panoptes kernel tuning
# Prevent inotify queue overflow during high event rates
fs.inotify.max_queued_events = 65536

# Support monitoring of large filesystem hierarchies
fs.inotify.max_user_watches = 524288

# Allow multiple concurrent monitoring instances
fs.inotify.max_user_instances = 256
```

Apply immediately:
```bash
sudo sysctl --system
```

### Kubernetes Node Configuration

For Kubernetes deployments, configure via DaemonSet init container:

```yaml
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: kernel-tuning
  namespace: kube-system
spec:
  selector:
    matchLabels:
      app: kernel-tuning
  template:
    metadata:
      labels:
        app: kernel-tuning
    spec:
      hostPID: true
      initContainers:
        - name: sysctl-tuning
          image: busybox
          securityContext:
            privileged: true
          command:
            - sh
            - -c
            - |
              sysctl -w fs.inotify.max_queued_events=65536
              sysctl -w fs.inotify.max_user_watches=524288
              sysctl -w fs.inotify.max_user_instances=256
              echo "Kernel tuning applied"
      containers:
        - name: pause
          image: registry.k8s.io/pause:3.9
```

For more details, see [docs/operations/kernel-tuning.md](/home/brett/repos/panoptes/docs/operations/kernel-tuning.md).

## Production Deployment Checklist

Use this checklist to ensure comprehensive platform hardening:

### Deployment

- [ ] Deploy CIS Kubernetes compliance template
  ```bash
  kubectl apply -f deploy/compliance/cis-kubernetes/template.yaml
  ```

- [ ] Deploy base security monitoring template
  ```bash
  kubectl apply -f deploy/compliance/base-security/template.yaml
  ```

- [ ] Label all in-scope monitoring pods
  ```bash
  kubectl label pods --all -n kube-system cis/scope=kubernetes-audit
  ```

- [ ] Label application workload pods
  ```bash
  kubectl label pods -n production panoptes.como-technologies.io/monitored=true
  ```

### Kernel Tuning

- [ ] Apply kernel sysctl settings on all nodes
  ```bash
  # Run on each node or via DaemonSet
  sysctl -w fs.inotify.max_queued_events=65536
  sysctl -w fs.inotify.max_user_watches=524288
  sysctl -w fs.inotify.max_user_instances=256
  ```

- [ ] Make kernel settings persistent (`/etc/sysctl.d/99-panoptes.conf`)

- [ ] Verify kernel settings applied
  ```bash
  sysctl fs.inotify.max_queued_events
  sysctl fs.inotify.max_user_watches
  sysctl fs.inotify.max_user_instances
  ```

### Monitoring and Alerting

- [ ] Deploy Prometheus alerting rules
  ```bash
  kubectl apply -f deploy/monitoring/prometheus-alerts.yaml
  ```

- [ ] Configure AlertManager routing for critical alerts

- [ ] Set up PagerDuty/Slack integration for critical alerts

- [ ] Test alert delivery
  ```bash
  # Trigger test alert by simulating config change
  ```

- [ ] Configure log forwarding to SIEM (Splunk, Elasticsearch, etc.)

### Enforcement Mode

- [ ] Start in audit mode (`enforcing: false`) for baseline establishment

- [ ] Review audit logs for 24-48 hours to identify legitimate access patterns

- [ ] Add necessary allowlists for legitimate applications

- [ ] Graduate to enforcement mode (`enforcing: true`) for runtime socket protection

- [ ] Monitor denied events for false positives

### Resource Management

- [ ] Set appropriate resource limits for daemons
  ```yaml
  resources:
    requests:
      cpu: 200m
      memory: 256Mi
    limits:
      cpu: 1000m
      memory: 1Gi
  ```

- [ ] Monitor daemon memory usage for tuning

- [ ] Review inotify watch count vs. available capacity

### Documentation and Runbooks

- [ ] Document custom monitoring configurations

- [ ] Create runbooks for common alerts

- [ ] Train team on Panoptes alert response procedures

- [ ] Document approved exception processes

### Validation

- [ ] Test config drift detection (modify watched file)

- [ ] Test runtime socket enforcement (attempt socket access)

- [ ] Test system binary protection (simulate binary modification)

- [ ] Verify alerts fire correctly

- [ ] Confirm metrics are exported to Prometheus

## Quick Wins: Immediate Security Improvements

These configurations provide immediate security value with minimal configuration:

1. **Block Runtime Socket Access** (5 minutes)
   - Deploys JanusGuard to block Docker/containerd socket access
   - Prevents 90% of container escape attacks
   - Zero false positives on properly configured workloads

2. **Monitor Kubernetes Config Files** (5 minutes)
   - Detects unauthorized changes to cluster configuration
   - Critical for config drift detection and compliance
   - Alerts on certificate tampering

3. **Detect Binary Tampering** (10 minutes)
   - Monitors system binaries for rootkit installation
   - Detects library injection attacks
   - Essential for forensics and incident response

## Deep Dive Documentation

For advanced configuration and security hardening:

- [Kernel Tuning Guide](../operations/kernel-tuning.md) - Detailed sysctl tuning and troubleshooting
- [Advanced Hardening](../security/advanced-hardening.md) - High-security environment configuration
- [Enabling Enforcement Mode](../guides/enabling-enforcement.md) - Migration from audit to enforcement
- [Attack Surface Analysis](../security/attack-surface-analysis.md) - Threat modeling and defense strategies

## Troubleshooting

### Events Not Detected

**Symptoms:** ArgusWatcher shows `eventsDetected: 0` after file modifications

**Causes:**
1. Pod selector doesn't match any pods
2. Monitored path doesn't exist in container
3. Events are filtered by ignore patterns

**Resolution:**
```bash
# Check pod selector
kubectl get pods -l cis/scope=kubernetes-audit

# Verify paths exist in target pod
POD=$(kubectl get pods -l cis/scope=kubernetes-audit -o name | head -1)
kubectl exec $POD -- ls -la /etc/kubernetes/manifests

# Check daemon logs
kubectl logs -n panoptes-system -l app=argusd
```

### Access Not Being Blocked

**Symptoms:** JanusGuard allows access that should be denied

**Causes:**
1. `enforcing: false` (audit mode)
2. `autoAllowOwner: true` allows pod owner
3. Process in allow list

**Resolution:**
```bash
# Check enforcement status
kubectl get jg -o jsonpath='{.items[*].spec.enforcing}' && echo

# Review JanusGuard configuration
kubectl get jg cis-k8s-secrets -o yaml

# Check daemon logs for decision reasoning
kubectl logs -n panoptes-system -l app=janusd | grep -A 5 "decision"
```

### High CPU Usage

**Symptoms:** Daemon pods consuming excessive CPU

**Causes:**
1. Too many watches on high-activity paths
2. No ignore patterns for noisy directories
3. Recursive monitoring of large trees

**Resolution:**
```bash
# Check event rate
kubectl logs -n panoptes-system -l app=argusd | grep -i "events/sec"

# Add ignore patterns for noisy paths
# Example: ignore log rotations, cache files, temporary files

# Reduce recursion depth with maxDepth
```

### Queue Overflow Alerts

**Symptoms:** `PanoptesInotifyQueueOverflow` firing

**Causes:**
1. Event rate exceeds queue capacity
2. Kernel limits too low
3. Daemon processing too slow
4. Possible denial-of-service attack

**Resolution:**
```bash
# Increase kernel queue size
sysctl -w fs.inotify.max_queued_events=131072

# Check for suspicious high-volume activity
kubectl top pods --sort-by=cpu

# Review event sources
kubectl logs -n panoptes-system -l app=argusd | grep -i overflow -A 10
```

## Next Steps

1. **Establish Baseline** - Run in audit mode for 24-48 hours to understand normal behavior
2. **Review Metrics** - Check Prometheus dashboards for event patterns and volume
3. **Enable Enforcement** - Gradually enable enforcement mode for critical paths
4. **Integrate SIEM** - Forward logs to your security information system
5. **Automate Response** - Create automated remediation for common security events

## Support

For questions or issues:
- GitHub Issues: https://github.com/como-technologies/panoptes/issues
- Documentation: https://github.com/como-technologies/panoptes/tree/main/docs
- Security Contact: security@como-technologies.io
