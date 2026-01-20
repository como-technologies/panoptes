# Audit Logging

> **Time:** 5 min (quick start) | 30+ min (deep dive)

Comprehensive file access logging for compliance, forensics, and security monitoring.

## Problem Statement

### The Challenge

Compliance frameworks and security best practices require maintaining detailed audit logs of who accessed what data and when. Traditional logging solutions:
- Don't capture file-level access in containers
- Lack process attribution (who/what accessed the file)
- Generate too much noise without intelligent filtering

### Who Needs This

- **Compliance teams** meeting HIPAA, SOC 2, GDPR audit requirements
- **Security teams** investigating incidents with access trails
- **Data governance** teams tracking access to sensitive data
- **Forensic analysts** reconstructing attack timelines

### Compliance Requirements

| Framework | Requirement | What's Needed |
|-----------|-------------|---------------|
| HIPAA | 164.312(b) | Audit controls recording PHI access |
| SOC 2 | CC6.1 | Logical access security controls |
| GDPR | Article 30 | Records of processing activities |
| PCI-DSS | 10.2 | Audit trail for all system components |

---

## Quick Start (5 Minutes)

### Step 1: Label Your Pods (30 seconds)

```bash
# Label pods that handle sensitive data
kubectl label pods -l app=data-service audit/scope=sensitive-data
```

### Step 2: Apply Audit Logging Configuration (30 seconds)

```bash
kubectl apply -f - <<'EOF'
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: audit-logger
  labels:
    use-case: audit-logging
spec:
  selector:
    matchLabels:
      audit/scope: "sensitive-data"
  subjects:
    # Data directory access
    - allow:
        - /data
        - /app/data
        - /var/lib/app
      events:
        - access
        - open
        - modify
      audit: true
      tags:
        category: data-access
        severity: medium

    # Configuration access
    - allow:
        - /etc
        - /app/config
      events:
        - access
        - open
      audit: true
      tags:
        category: config-access
        severity: low

    # Credential files (audit all access)
    - allow:
        - /etc/passwd
        - /etc/group
      deny:
        - /etc/shadow
      events:
        - access
        - open
      audit: true
      tags:
        category: credential-access
        severity: high

  containerRuntime: auto
  enforcing: false  # Audit only, don't block
  paused: false
  logFormat: json
EOF
```

### Step 3: Verify It's Working (2 minutes)

```bash
# Check JanusGuard status
kubectl get janusguard audit-logger -o wide

# Generate test access events
POD=$(kubectl get pods -l audit/scope=sensitive-data -o jsonpath='{.items[0].metadata.name}')
kubectl exec $POD -- cat /etc/passwd
kubectl exec $POD -- ls /data 2>/dev/null || echo "No /data dir"
```

### Step 4: View in Dashboard (1 minute)

```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

Navigate to **Events** page and filter by `category: data-access`

---

## What Success Looks Like

### Expected Audit Events

| Event Type | Path | Category | Meaning |
|------------|------|----------|---------|
| `open` | `/data/customers.db` | data-access | Database file opened |
| `access` | `/app/config/secrets.yaml` | config-access | Config file read |
| `modify` | `/data/reports/Q4.xlsx` | data-access | Report file modified |
| `open` | `/etc/passwd` | credential-access | User list accessed |

### Audit Event Structure

Each event includes:

```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "guardName": "audit-logger",
  "podName": "data-service-abc123",
  "containerName": "app",
  "eventType": "open",
  "path": "/data/customers.db",
  "action": "audit",
  "processName": "python",
  "processId": 1234,
  "tags": {
    "category": "data-access",
    "severity": "medium"
  }
}
```

---

## Deep Dive

### Combined FIM + Access Logging

For complete audit trails, combine ArgusWatcher (changes) with JanusGuard (access):

```yaml
# ArgusWatcher - Track changes
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: audit-fim
spec:
  selector:
    matchLabels:
      audit/scope: "sensitive-data"
  subjects:
    - paths:
        - /data
      events:
        - create
        - modify
        - delete
      recursive: true
      tags:
        audit-type: modification
---
# JanusGuard - Track access
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: audit-access
spec:
  selector:
    matchLabels:
      audit/scope: "sensitive-data"
  subjects:
    - allow:
        - /data
      events:
        - access
        - open
      audit: true
      tags:
        audit-type: access
  enforcing: false
```

This provides:
- **Who read the file** (JanusGuard access events)
- **Who modified the file** (ArgusWatcher modify events)
- **When files were created/deleted** (ArgusWatcher create/delete events)

### Process Attribution

Enable detailed process information in audit logs:

```yaml
spec:
  subjects:
    - allow:
        - /data
      events:
        - access
        - open
      audit: true
      # Process info is captured automatically when available
      tags:
        include-process-info: "true"
```

Audit logs will include:
- Process name (e.g., `python`, `java`)
- Process ID (PID)
- Executable path (when available)

### Regulated Data Monitoring

#### HIPAA - Protected Health Information (PHI)

```yaml
spec:
  selector:
    matchLabels:
      hipaa/scope: "ephi"
  subjects:
    # Patient data directories
    - allow:
        - /data/patients
        - /app/ehr
        - /var/lib/medical
      events:
        - access
        - open
        - modify
      audit: true
      tags:
        data-type: phi
        regulation: hipaa
        requirement: "164.312(b)"
```

#### GDPR - Personal Data

```yaml
spec:
  selector:
    matchLabels:
      gdpr/scope: "personal-data"
  subjects:
    # EU citizen data
    - allow:
        - /data/eu-customers
        - /app/pii
      events:
        - access
        - open
        - modify
      audit: true
      tags:
        data-type: pii
        regulation: gdpr
        article: "30"

    # Data export monitoring
    - allow:
        - /export
        - /backup
      events:
        - all
      audit: true
      tags:
        data-type: export
        regulation: gdpr
        article: "20"  # Data portability
```

### Audit Log Retention

Export audit logs for long-term retention:

#### Real-time Export to SIEM

```yaml
# Configure OpenTelemetry export
observability:
  opentelemetry:
    enabled: true
    endpoint: "otel-collector.monitoring:4317"
    protocol: grpc
```

#### Periodic Export

```bash
# Export last 24 hours of audit events
curl "http://localhost:3000/api/events?since=24h&tags=audit-type" > audit-$(date +%Y%m%d).json

# Export to CSV for compliance reports
curl "http://localhost:3000/api/events?format=csv&since=24h" > audit-$(date +%Y%m%d).csv
```

#### Dashboard Export

1. Navigate to **Events** page
2. Apply filters (date range, tags)
3. Click **Export CSV** or **Export JSON**

### Alerting on Unusual Access Patterns

```yaml
groups:
  - name: panoptes-audit-alerts
    rules:
      # Alert on high-volume data access
      - alert: UnusualDataAccessVolume
        expr: increase(panoptes_janus_events_total{tags_category="data-access"}[1h]) > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Unusual volume of data access events"
          description: "{{ $value }} access events in the last hour from {{ $labels.pod }}"

      # Alert on after-hours access
      - alert: AfterHoursDataAccess
        expr: panoptes_janus_events_total{tags_category="data-access"} > 0 and hour() < 6 or hour() > 22
        for: 0m
        labels:
          severity: warning
        annotations:
          summary: "Data accessed outside business hours"

      # Alert on sensitive credential access
      - alert: CredentialFileAccessed
        expr: increase(panoptes_janus_events_total{tags_category="credential-access", path=~".*/shadow.*"}[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Sensitive credential file accessed"
```

### Operational Considerations

#### Event Volume Management

Audit logging can generate high event volumes. Control this with:

1. **Selective path monitoring** - Only monitor paths with sensitive data
2. **Event type filtering** - Use `open` instead of `access` for less noise
3. **Process filtering** - Ignore known-good processes (future feature)

```yaml
subjects:
  - allow:
      - /data/sensitive-only  # Narrow scope
    events:
      - open  # Not 'access' which fires more often
    audit: true
```

#### Storage Requirements

Estimate storage needs:
- ~500 bytes per audit event (JSON)
- 10,000 events/day = ~5 MB/day = ~150 MB/month

For high-volume environments:
- Use streaming export to SIEM
- Set retention policies
- Archive older logs to object storage

#### Compliance Report Generation

Create audit reports for compliance reviews:

```bash
# Generate monthly compliance report
curl "http://localhost:3000/api/events?since=30d&tags=regulation:hipaa" \
  | jq '{
    period: "2024-01",
    total_events: length,
    by_category: group_by(.tags.category) | map({key: .[0].tags.category, count: length}),
    unique_pods: [.[].podName] | unique | length
  }' > hipaa-audit-report-2024-01.json
```

---

## Troubleshooting

### Events Not Appearing

1. **Verify JanusGuard selector matches pods:**
   ```bash
   kubectl get pods -l audit/scope=sensitive-data
   ```

2. **Check JanusGuard is not paused:**
   ```bash
   kubectl get janusguard audit-logger -o jsonpath='{.spec.paused}'
   ```

3. **Verify janusd daemon is running:**
   ```bash
   kubectl get pods -n panoptes-system -l app=janusd
   ```

### Missing Process Information

Process attribution requires:
- Container runtime support (containerd, CRI-O)
- Sufficient daemon privileges (`CAP_SYS_PTRACE`)

Check daemon logs:
```bash
kubectl logs -n panoptes-system -l app=janusd | grep -i "process"
```

### High Event Volume

Reduce noise by:
1. Narrowing monitored paths
2. Using `open` instead of `access` events
3. Adding ignore patterns (when supported)

---

## Related Documentation

- [Compliance Monitoring](compliance-monitoring.md) - Framework-specific compliance
- [Security Incident Detection](security-incident-detection.md) - Detecting threats
- [What to Monitor](../what-to-monitor.md) - Path recommendations by data type
