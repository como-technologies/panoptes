# Enabling Enforcement Mode

This guide covers how to safely enable enforcement mode on JanusGuard resources to actively block policy violations rather than just logging them.

## Overview

JanusGuard supports two operational modes:

| Mode | `enforcing` | Behavior | Use Case |
|------|-------------|----------|----------|
| **Audit** | `false` | Log violations, allow access | Testing, baseline collection |
| **Enforcement** | `true` | Log violations, block access | Production security |

By default, all compliance templates ship with `enforcing: false` to prevent accidental service disruption. Only CIS-Kubernetes templates enable enforcement by default for container runtime socket protection.

## When to Enable Enforcement

Enable enforcement when:

1. You've run in audit mode for at least 1-2 weeks
2. You've reviewed all logged violations and confirmed they're not false positives
3. You've added necessary allowlists for legitimate access patterns
4. You have rollback procedures in place
5. You've tested in a non-production environment first

## Security Implications of Audit-Only Mode

Running in audit-only mode means:

- **Attacks are logged but NOT blocked** - An attacker reading `/etc/shadow` will succeed
- **Compliance may be incomplete** - Some frameworks require active prevention, not just detection
- **Forensic value only** - You'll know an attack happened, but won't have prevented it

For production environments handling sensitive data, enforcement mode is strongly recommended.

## Gradual Rollout Strategy

### Week 1: Baseline Collection (Audit Mode)

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: my-guard
spec:
  enforcing: false  # Audit mode
  subjects:
    - deny:
        - /etc/shadow
      events: [access, open]
      audit: true
```

During this phase:
- Monitor the Events dashboard daily
- Document all unique access patterns
- Identify legitimate processes that need access

### Week 2: Analysis and Allowlisting

Review collected events and create allowlists:

```bash
# Export events for analysis
curl "http://localhost:3000/api/events?guard=my-guard&since=7d" | \
  jq -r '.[] | "\(.process) -> \(.path)"' | sort | uniq -c | sort -rn
```

Common legitimate access patterns to allowlist:
- `/etc/shadow` - `login`, `sshd`, `sudo`, `su`, PAM modules
- `/etc/sudoers` - `sudo`, `visudo`
- SSH keys - `sshd`, `ssh-agent`

### Week 3: Selective Enforcement

Enable enforcement on high-confidence rules first:

```yaml
spec:
  enforcing: false  # Global default still audit
  subjects:
    # High confidence - enable enforcement
    - deny:
        - /var/run/docker.sock
        - /var/run/containerd/containerd.sock
      events: [access, open]
      enforcing: true  # Per-subject override
      audit: true
      tags:
        reason: "Container escape prevention"

    # Lower confidence - keep audit mode
    - deny:
        - /etc/shadow
      events: [access, open]
      enforcing: false  # Still auditing
      audit: true
```

### Week 4: Full Enforcement

After validating selective enforcement works:

```yaml
spec:
  enforcing: true  # Global enforcement
  subjects:
    # All subjects now enforce by default
    - deny:
        - /etc/shadow
      events: [access, open]
      audit: true
```

## Enabling Enforcement

### Method 1: Patch Existing Resource

```bash
# Enable enforcement on a specific JanusGuard
kubectl patch janusguard my-guard --type=merge -p '{"spec":{"enforcing":true}}'

# Verify
kubectl get janusguard my-guard -o jsonpath='{.spec.enforcing}'
```

### Method 2: Edit YAML

```bash
kubectl edit janusguard my-guard
# Change: enforcing: false
# To:     enforcing: true
```

### Method 3: Apply Updated Manifest

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: my-guard
spec:
  enforcing: true  # Changed from false
  # ... rest of spec
```

```bash
kubectl apply -f my-guard.yaml
```

## Rollback Procedures

### Immediate Rollback (Emergency)

If enforcement causes service disruption:

```bash
# Disable enforcement immediately
kubectl patch janusguard my-guard --type=merge -p '{"spec":{"enforcing":false}}'

# Or pause the guard entirely
kubectl patch janusguard my-guard --type=merge -p '{"spec":{"paused":true}}'
```

### Rollback Specific Rules

If only certain rules are problematic:

```bash
kubectl patch janusguard my-guard --type=json -p='[
  {"op": "replace", "path": "/spec/subjects/0/enforcing", "value": false}
]'
```

### Full Rollback

```bash
# Delete and recreate with audit mode
kubectl delete janusguard my-guard
kubectl apply -f my-guard-audit-mode.yaml
```

## Impact on Application Availability

### How Enforcement Works

When enforcement is enabled and a process attempts to access a denied path:

1. Kernel sends `FAN_OPEN_PERM` or `FAN_ACCESS_PERM` event to janusd
2. janusd evaluates the policy
3. If denied: janusd responds with `FAN_DENY`, kernel returns `EACCES` to process
4. Process receives "Permission denied" error

### Potential Impacts

| Scenario | Impact | Mitigation |
|----------|--------|------------|
| Legitimate process blocked | Application error/crash | Add to allowlist before enforcement |
| Startup dependency blocked | Pod fails to start | Test in staging first |
| Health check blocked | Pod marked unhealthy | Exclude health check paths |
| Backup process blocked | Backup failure | Schedule enforcement windows |

### High Availability Considerations

```yaml
# For critical workloads, consider per-pod gradual rollout
spec:
  selector:
    matchLabels:
      app: critical-service
      enforcement-canary: "true"  # Only enforce on canary pods first
```

## Monitoring Enforcement

### Prometheus Metrics

```promql
# Denied events rate
rate(panoptes_janus_events_total{action="deny"}[5m])

# Denied events by path
sum by (path) (increase(panoptes_janus_events_total{action="deny"}[1h]))
```

### Alerting

```yaml
groups:
  - name: panoptes-enforcement
    rules:
      - alert: HighDenyRate
        expr: rate(panoptes_janus_events_total{action="deny"}[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High deny rate detected"
          description: "JanusGuard is denying more than 10 requests/sec"

      - alert: CriticalPathDenied
        expr: increase(panoptes_janus_events_total{action="deny", tags_severity="critical"}[5m]) > 0
        labels:
          severity: critical
        annotations:
          summary: "Critical path access denied"
```

## Troubleshooting

### Application Failing After Enforcement

1. Check which paths are being denied:
   ```bash
   kubectl logs -n panoptes-system -l app=janusd | grep -i deny
   ```

2. Identify the process:
   ```bash
   kubectl logs -n panoptes-system -l app=janusd | jq 'select(.action=="deny") | {path, process, pid}'
   ```

3. If legitimate, add to allowlist or disable enforcement temporarily

### Enforcement Not Working

1. Verify enforcement is enabled:
   ```bash
   kubectl get janusguard my-guard -o jsonpath='{.spec.enforcing}'
   ```

2. Check daemon is running with correct capabilities:
   ```bash
   kubectl get pods -n panoptes-system -l app=janusd -o yaml | grep -A5 capabilities
   ```

3. Verify fanotify permissions:
   ```bash
   kubectl exec -n panoptes-system -l app=janusd -- cat /proc/self/status | grep Cap
   ```

### Performance Impact

Enforcement adds latency to file operations on monitored paths. If experiencing performance issues:

1. Reduce the number of monitored paths
2. Use more specific path patterns instead of broad directories
3. Increase daemon resources:
   ```yaml
   resources:
     requests:
       cpu: 200m
       memory: 256Mi
     limits:
       cpu: 500m
       memory: 512Mi
   ```

## Template-Specific Guidance

### CIS Kubernetes

Already has enforcement enabled for runtime sockets. Recommended to keep enabled.

### PCI-DSS, HIPAA, SOC2, NIST

Ship with enforcement disabled. For compliance:
- PCI-DSS Req 7.1: Consider enabling for credential file protection
- HIPAA: Consider enabling for PHI data paths
- SOC2: Consider enabling for audit log protection

### GDPR

Data access auditing may be sufficient, but enforcement on personal data paths adds protection.

## Best Practices

1. **Never enable enforcement without testing** - Always run audit mode first
2. **Start with high-confidence rules** - Container runtime sockets, credential files
3. **Have rollback ready** - Document and test rollback procedures before enforcement
4. **Monitor closely after enabling** - Watch for increased error rates
5. **Communicate with teams** - Warn application teams before enabling enforcement
6. **Use gradual rollout** - Enable on canary pods before fleet-wide

## Related Documentation

- [Attack Surface Analysis](../security/attack-surface-analysis.md) - Understanding what's protected
- [Remediation Plan](../security/remediation-plan.md) - Security improvement tracking
- [Advanced Hardening](../security/advanced-hardening.md) - Additional security measures
