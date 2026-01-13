# Panoptes Future State: Security Monitoring Without the Bloat

> "The manual page, which really used to be a manual page, is now a small volume, with a thousand options... We used to sit around in the Unix Room saying, 'What can we throw out?'"
> — Doug McIlroy, Bell Labs

---

## Philosophy: One Tool, One Job, Done Well

Panoptes follows the Unix philosophy of doing one thing exceptionally well. While enterprise security platforms have become bloated with AI/ML detection, thousands of pre-built policies, auto-remediation engines, and dashboard sprawl, we take the opposite approach:

**Tried. Tested. Obvious. Explainable.**

Every feature in Panoptes has a clear purpose. Every alert has a clear cause. Every log entry tells you exactly what happened. This isn't security theater—it's security for security professionals.

### Core Principles

1. **Kernel-level detection**: We use Linux inotify and fanotify—the same mechanisms the kernel uses. No heuristics. No guessing. If a file changed, we know because the kernel told us.

2. **Kubernetes-native**: CRDs, operators, and label selectors. Not YAML soup or agent sprawl.

3. **Transparent operation**: Clear logs, obvious behavior. You can explain any alert to an auditor in one sentence.

4. **Expert-focused**: Built for security professionals who understand what they're monitoring. Not for sales demos.

5. **Composable**: Works with your existing observability stack. Prometheus. Grafana. Loki. Your SIEM. We don't try to replace them.

---

## Current State: What Panoptes Does Today

### Argus: File Integrity Monitoring

| Capability | Implementation |
|------------|----------------|
| Real-time file change detection | Linux inotify |
| Event types | create, modify, delete, access, open, close, move, attrib |
| Recursive directory watching | Yes, with configurable max depth |
| Container-aware | PID namespace resolution via containerd/CRI-O |
| Kubernetes-native | ArgusWatcher CRD with label selectors |
| Pausing | `spec.paused` field for maintenance windows |

### Janus: File Access Auditing

| Capability | Implementation |
|------------|----------------|
| Real-time access monitoring | Linux fanotify |
| Access blocking | FAN_DENY response for denied paths |
| Allow/deny policies | Per-subject configuration |
| Kernel audit integration | AUDIT_WRITE capability |
| Dry-run mode | `spec.enforcing: false` |
| Container-aware | Same as Argus |
| Kubernetes-native | JanusGuard CRD with label selectors |

### Observability

| Capability | Status |
|------------|--------|
| Prometheus metrics | Implemented |
| Structured JSON logs | Implemented |
| OpenTelemetry export | Optional |
| Grafana dashboards | Pre-built templates |
| Alert rules | PrometheusRule resources |

---

## Gap Analysis: Essential Compliance Features

### What Compliance Frameworks Actually Require

After researching PCI-DSS 4.0, HIPAA, SOC2, NIST 800-53, and GDPR requirements, here's what auditors actually need:

| Framework | Section | Actual Requirement |
|-----------|---------|-------------------|
| **PCI-DSS 4.0** | 10.3.4 | FIM on audit logs to detect unauthorized changes |
| **PCI-DSS 4.0** | 11.5.2 | Alert on unauthorized modification of critical files, compare weekly minimum |
| **HIPAA** | Security Rule | Data integrity protection, audit trail maintenance |
| **SOC2** | CC6.1 | Logical access controls, change detection |
| **NIST 800-53** | SI-7 | Software/firmware/info integrity verification |
| **GDPR** | Art. 32 | Security of processing, integrity assurance |

**Key insight:** Auditors don't need fancy dashboards. They need:

1. Proof that critical files are monitored
2. Logs showing what changed, when, and by whom
3. Evidence that alerts fire on unauthorized changes
4. Retention (13 months for PCI-DSS)
5. Exportable reports they can attach to audit findings

---

## Compliance Control Mapping

The table below maps planned Panoptes features to specific compliance framework requirements:

| Feature | PCI-DSS 4.0 | HIPAA | SOC2 | NIST 800-53 | GDPR |
|---------|-------------|-------|------|-------------|------|
| Process Attribution | 10.3.4, 11.5.2 | 164.312(b) | CC6.1, CC7.2 | AU-2, AU-3 | Art. 32 |
| Content Hashing | 11.5.2 | 164.312(c)(1) | CC6.1 | SI-7(1) | Art. 32 |
| Compliance Reporting | 11.5.2 | 164.312(b) | CC7.2 | AU-6 | Art. 30 |
| Event Retention | 10.7 | 164.530(j) | CC7.1 | AU-4 | Art. 5(1)(e) |
| Cluster & Fleet Context | 10.3.4, 11.5.2 | 164.312(b) | CC6.1, CC7.2 | AU-2, AU-3 | Art. 32 |
| Baseline Snapshots | 11.5.2 | 164.312(c)(1) | CC6.1 | SI-7(2) | Art. 32 |

### PCI-DSS 4.0 Controls

- **10.3.4**: File integrity monitoring on audit logs to detect unauthorized changes
- **10.7**: Retain audit trail history for at least 12 months (13 months recommended)
- **11.5.2**: Alert on unauthorized modification of critical files; compare weekly minimum

### HIPAA Security Rule (45 CFR 164.312)

- **164.312(a)(2)(i)**: Access controls—technical policies for authorized access to ePHI
- **164.312(b)**: Audit controls—log and examine activity in systems with ePHI
- **164.312(c)(1)**: Integrity controls—ensure ePHI is not altered or destroyed
- **164.530(j)**: Retain documentation for 6 years from creation or last effective date

### SOC2 Trust Criteria

- **CC6.1**: Logical access security over protected information assets
- **CC7.1**: System monitoring to detect configuration changes and anomalies
- **CC7.2**: Monitor system components for anomalies indicating malicious acts

### NIST 800-53 Rev. 5

- **AU-2/AU-3**: Audit events—determine auditable events and content of audit records
- **AU-4**: Audit storage capacity
- **AU-6**: Audit review, analysis, and reporting
- **SI-7**: Software, firmware, and information integrity
- **SI-7(1)**: Integrity verification using cryptographic hash functions
- **SI-7(2)**: Automated response to integrity violations with baseline comparison

### GDPR

- **Article 5(1)(e)**: Storage limitation—data kept no longer than necessary
- **Article 30**: Records of processing activities
- **Article 32**: Security of processing—integrity and confidentiality measures

---

## Priority 1: Must Have (Essential for Compliance)

### 1. Process Attribution

**Gap:** We detect changes but not WHO or WHAT caused them.

**Required:** User ID, process name, PID, command line of the process that made the change.

**Implementation Path:**

**Janus (fanotify-based):**
- Extend fanotify with FAN_REPORT_FID and FAN_REPORT_PIDFD
- Add `/proc/{pid}/cmdline` lookup for process context
- Include in event metadata: uid, gid, process name, cmdline, cwd
- Straightforward - fanotify provides PID directly in event metadata

**Argus (inotify-based):**
- **Key limitation:** inotify does NOT provide process attribution. It only reports WHAT changed, not WHO changed it.
- **Option A (Recommended):** Use Linux Audit subsystem (`auditd`) to correlate file changes with process info. Enable audit rules for monitored paths and join inotify events with audit records.
- **Option B:** Add eBPF hooks to VFS operations (vfs_write, vfs_unlink, etc.) to capture process context alongside file changes. Higher implementation complexity but better performance.
- **Option C:** Replace inotify with fanotify for Argus FIM. Gains process attribution but loses inotify's simpler recursive watching semantics.

| Approach | Complexity | Performance | Attribution Quality |
|----------|------------|-------------|---------------------|
| Audit correlation | Medium | Medium (audit overhead) | Full |
| eBPF VFS hooks | High | High | Full |
| Switch to fanotify | Medium | High | Full |

**Why:** PCI-DSS and all major frameworks require "who made the change" for audit trails. Without process attribution, FIM evidence is incomplete.

**Example Output:**
```json
{
  "event": "modify",
  "path": "/etc/passwd",
  "timestamp": "2026-01-10T14:32:00Z",
  "attribution": {
    "uid": 0,
    "gid": 0,
    "user": "root",
    "process": "useradd",
    "pid": 12345,
    "cmdline": "useradd -m newuser",
    "cwd": "/root"
  }
}
```

### 2. Content Hashing (Baseline Verification)

**Gap:** We detect that a file was modified but not whether the content actually changed or matches a known-good state.

**Required:** Cryptographic hash comparison against a baseline.

#### Security Considerations

> **WARNING:** Storing plaintext SHA-256 hashes of sensitive files in ConfigMaps or CRD
> annotations is a **security anti-pattern**. This enables confirmation attacks and
> violates the security intent of compliance frameworks.

**Why plaintext hashes are dangerous:**

1. **Confirmation Attacks**: Attacker with ConfigMap read access can pre-compute hashes
   against known file contents (e.g., password databases) and verify their guesses
   without ever reading the protected file.

2. **Information Leakage**: Hash changes reveal when users are added/removed, when
   password rotations occur, and which users exist across a fleet.

3. **ConfigMaps Are Not Encrypted**: Visible in `kubectl get configmap -o yaml`,
   logged in audit trails, exposed in etcd backups.

**Industry Practice**: No enterprise FIM tool (OSSEC, Wazuh, Tripwire) stores baselines
in plaintext shared storage. All use encrypted databases or local filesystem with
restricted permissions.

#### Recommended Approaches

**Option A: HMAC-SHA256 (Recommended)**

Use keyed hashing—attacker cannot pre-compute without the secret key:

```yaml
subjects:
  - paths: ["/etc/passwd"]
    events: [modify]
    hashBaseline: true
    hashAlgorithm: hmac-sha256
    hmacKeySecret:
      name: argus-hmac-key
      key: key
    verifyInterval: 1h
```

The HMAC key is stored in a Kubernetes Secret (encrypted at rest if etcd encryption
is enabled), not a ConfigMap. Key rotation is independent of baselines.

**Option B: Exclude Sensitive Files from Hashing**

For files containing credentials (`/etc/shadow`, private keys), use event detection
only—don't attempt to verify content:

```yaml
subjects:
  - paths: ["/etc/shadow"]
    events: [modify, delete]
    hashBaseline: false  # Detect changes, don't hash contents
    tags:
      severity: critical
```

This is appropriate when:
- File contains credentials/secrets
- Change detection is sufficient (no need to verify "correct" state)
- Compliance only requires "detect modification" not "verify integrity"

**Option C: External Baseline Service (Production)**

For high-security environments, keep baselines off the Kubernetes cluster entirely:

```
Panoptes Daemon → (gRPC + mTLS) → External Baseline Service → HSM/KMS
```

Benefits: Baselines never in etcd, hardware-backed encryption, centralized audit
logging, FIPS 140-2 compliant.

#### Implementation Path

1. Add `BaselineConfig` to proto with HMAC support and `SecretKeyRef`
2. Operator validates HMAC key Secret exists before enabling baseline hashing
3. Daemon loads HMAC key from mounted Secret at startup
4. Use constant-time comparison to prevent timing attacks
5. Reject insecure configurations (plaintext SHA-256 in ConfigMap)

#### Summary

| Approach | Security | When to Use |
|----------|----------|-------------|
| SHA-256 in ConfigMap | **INSECURE** | Never |
| HMAC-SHA256 in Secret | Secure | Default for baseline verification |
| Skip hashing | Secure | For `/etc/shadow`, credential files |
| External service | Most secure | Production, regulated environments |

**Why:** Detects tampering even if timestamps are manipulated. Required for detecting
sophisticated attacks that bypass inotify. HMAC ensures baselines cannot be exploited
by attackers with partial cluster access.

### 3. Compliance-Ready Reporting

**Gap:** We output Prometheus metrics and raw events. Auditors need formatted reports.

**Required:** Framework-specific evidence documents.

**Implementation Path:**
- Add report generator to Panoptes Eye
- Templates for PCI-DSS, HIPAA, SOC2, NIST
- Export as PDF, CSV, JSON
- Scheduled report generation
- Include: monitored paths, event counts, alerts, coverage metrics

**Example Report Structure:**
```
PANOPTES FILE INTEGRITY MONITORING
Compliance Evidence Report

Framework: PCI-DSS 4.0
Period: 2025-12-01 to 2026-01-10
Generated: 2026-01-10 14:32:00 UTC

REQUIREMENT 11.5.2 - FILE INTEGRITY MONITORING
═══════════════════════════════════════════════

Monitored Systems: 47 pods across 3 namespaces
Active Watchers: 12 ArgusWatcher resources
Monitoring Mode: Real-time (continuous)

Critical Paths Monitored:
  ✓ /etc/passwd (47/47 pods)
  ✓ /etc/shadow (47/47 pods)
  ✓ /etc/sudoers (47/47 pods)
  ✓ /usr/bin/* (47/47 pods)

Events Summary:
  Total Events: 1,247
  - Create: 312
  - Modify: 891
  - Delete: 44

Alerts Generated: 3
  - CriticalFileModified: 2
  - UnauthorizedAccess: 1

[Detailed event log: Appendix A]
```

### 4. Event Retention & Export

**Gap:** Events flow to external SIEM/logging. Some orgs lack mature log infrastructure.

**Required:** Built-in retention option with easy export.

**Implementation Options:**

1. **External SIEM (Recommended for enterprises)**
   - Forward events to existing Splunk/Elastic/Datadog
   - Use their retention and compliance features
   - Panoptes focuses on detection, SIEM handles storage

2. **Built-in Storage (See Section 4.5 below)**
   - For orgs without mature log infrastructure
   - Compliance-ready retention policies
   - Optional, composable architecture

**Minimum Requirements:**
- 13-month retention for PCI compliance
- One-click CSV/JSON export
- API endpoint for programmatic access

### 4.5 Backend Storage Integration (Compliance-Ready)

**Philosophy:** Don't reinvent compliance infrastructure. Use proven, certified solutions.

Panoptes follows the Unix philosophy of composability. For event storage and retention,
we integrate with existing enterprise solutions rather than building custom compliance
infrastructure. Cloud providers and enterprise vendors have teams dedicated to maintaining
compliance certifications—leverage their investment.

#### Recommended: Enterprise SIEM Integration

For most organizations, the right answer is to forward Panoptes events to your existing
SIEM or observability platform. These tools already have:

- Compliance certifications (SOC2, HIPAA BAA, PCI-DSS)
- Retention policies and lifecycle management
- Search, alerting, and correlation capabilities
- Audit trails for data access

**Supported SIEM Endpoints:**

| Platform | Integration | Compliance |
|----------|-------------|------------|
| **Splunk** | HTTP Event Collector (HEC) | SOC2, HIPAA BAA, PCI-DSS |
| **Elastic/OpenSearch** | Direct indexing, Logstash | SOC2, HIPAA (with Shield) |
| **Datadog** | OTLP, Log forwarding | SOC2, HIPAA BAA, PCI-DSS |
| **Google Chronicle** | OTLP, Pub/Sub | FedRAMP, SOC2, HIPAA |
| **Azure Sentinel** | Log Analytics, Event Hub | SOC2, HIPAA BAA, ISO 27001 |
| **AWS Security Hub** | CloudWatch Logs, EventBridge | SOC2, HIPAA, PCI-DSS |

**Helm Configuration:**
```yaml
# values.yaml - SIEM forwarding
eventForwarding:
  enabled: true

  # Splunk HEC
  splunk:
    enabled: true
    endpoint: "https://splunk.example.com:8088"
    tokenSecretRef:
      name: splunk-hec-token
      key: token
    index: "panoptes-events"

  # Datadog
  datadog:
    enabled: false
    apiKeySecretRef:
      name: datadog-api-key
      key: api-key
    site: "datadoghq.com"

  # Generic OTLP (works with many platforms)
  otlp:
    enabled: false
    endpoint: "https://otel-collector:4317"
    headers:
      Authorization: "Bearer ${OTLP_TOKEN}"
```

**Why SIEM is the right choice:**
- Your security team already knows how to use it
- Correlation with other security data sources
- Existing dashboards and alert workflows
- Compliance certifications maintained by vendor
- No additional infrastructure to manage

#### Alternative: Managed Database Storage

For organizations without mature SIEM infrastructure, or for smaller deployments,
Panoptes can store events in a managed PostgreSQL database. **Do not self-host**
database infrastructure for compliance workloads—use cloud-managed services with
compliance certifications.

**Cloud Provider Options:**

| Provider | Service | Compliance Certifications |
|----------|---------|---------------------------|
| **AWS** | RDS for PostgreSQL | HIPAA BAA, SOC2, PCI-DSS, FedRAMP |
| **GCP** | Cloud SQL for PostgreSQL | HIPAA BAA, SOC2, ISO 27001 |
| **Azure** | Database for PostgreSQL | HIPAA BAA, SOC2, PCI-DSS |
| **Spectro Cloud** | PostgreSQL Add-on | Inherits cluster compliance |

**Helm Configuration:**
```yaml
# values.yaml - External managed PostgreSQL
storage:
  type: postgresql

  # Point to managed database (do NOT self-host)
  postgresql:
    external:
      enabled: true
      host: "panoptes-db.xxx.us-east-1.rds.amazonaws.com"
      port: 5432
      database: "panoptes"
      sslMode: "verify-full"  # Required for compliance
      credentialsSecretRef:
        name: panoptes-db-credentials
        usernameKey: username
        passwordKey: password
```

**Cloud Provider Setup Checklist:**

AWS RDS:
- [ ] Enable encryption at rest (KMS)
- [ ] Enable automated backups (35-day retention for PCI)
- [ ] Enable Performance Insights for audit
- [ ] Configure security group for cluster-only access
- [ ] Enable Enhanced Monitoring
- [ ] Sign HIPAA BAA if processing PHI

GCP Cloud SQL:
- [ ] Enable encryption (Google-managed or CMEK)
- [ ] Enable automated backups
- [ ] Configure private IP (no public access)
- [ ] Enable audit logging via Cloud Audit Logs
- [ ] Add to VPC Service Controls perimeter

Azure Database:
- [ ] Enable Azure Defender for PostgreSQL
- [ ] Configure private endpoint
- [ ] Enable Advanced Threat Protection
- [ ] Configure diagnostic logs to Log Analytics

#### Archive Storage (Cold Tier)

For long-term retention (HIPAA 6-year requirement), use cloud object storage with
**WORM (Write-Once-Read-Many)** policies. Cloud providers offer native immutability
features that meet compliance requirements without custom code.

**Cloud-Native WORM Options:**

| Provider | Feature | Compliance Mode |
|----------|---------|-----------------|
| **AWS S3** | Object Lock | Governance / Compliance mode |
| **GCP GCS** | Retention Policy | Bucket Lock (irreversible) |
| **Azure Blob** | Immutable Storage | Legal Hold / Time-based |

**AWS S3 Object Lock Configuration:**
```yaml
# Terraform/OpenTofu example
resource "aws_s3_bucket" "panoptes_archive" {
  bucket = "panoptes-archive-${var.environment}"

  object_lock_configuration {
    object_lock_enabled = "Enabled"
  }
}

resource "aws_s3_bucket_object_lock_configuration" "compliance" {
  bucket = aws_s3_bucket.panoptes_archive.id

  rule {
    default_retention {
      mode = "COMPLIANCE"  # Cannot be overridden, even by root
      years = 6            # HIPAA requirement
    }
  }
}
```

**Lifecycle Rules (Hot → Cold Tiering):**
```yaml
# S3 lifecycle rule - move to Glacier after 90 days
resource "aws_s3_bucket_lifecycle_configuration" "archive" {
  bucket = aws_s3_bucket.panoptes_archive.id

  rule {
    id     = "archive-to-glacier"
    status = "Enabled"

    transition {
      days          = 90
      storage_class = "GLACIER"
    }

    transition {
      days          = 365
      storage_class = "DEEP_ARCHIVE"
    }
  }
}
```

#### Spectro Cloud Palette Integration

For Spectro Cloud Palette deployments, use managed add-ons for infrastructure:

**PostgreSQL Add-on:**
```yaml
# Palette cluster profile pack
pack:
  name: postgresql
  version: "15.x"
  values: |
    persistence:
      enabled: true
      size: 100Gi
      storageClass: "encrypted-ssd"

    # Enable TLS
    tls:
      enabled: true
      certificatesSecret: panoptes-db-tls
```

**Logging Add-on (Fluentd/Fluent Bit):**
```yaml
# Forward Panoptes events to enterprise logging
pack:
  name: spectro-fluentbit
  values: |
    outputs: |
      [OUTPUT]
          Name        splunk
          Match       panoptes.*
          Host        splunk.example.com
          Port        8088
          TLS         On
          Splunk_Token ${SPLUNK_HEC_TOKEN}
```

#### Compliance Responsibility Matrix

| Compliance Requirement | Panoptes Responsibility | Platform/Cloud Responsibility |
|------------------------|-------------------------|-------------------------------|
| **Encryption at rest** | Configure TLS for connections | Enable storage encryption |
| **Encryption in transit** | Use TLS 1.3 for gRPC/SSE | Configure load balancer TLS |
| **Retention periods** | Forward events with timestamps | Configure retention policies |
| **Access logging** | Log queries to events API | Enable cloud audit logs |
| **Immutability (WORM)** | Export in append-only format | Enable Object Lock / Bucket Lock |
| **Backup & recovery** | Export functionality | Automated backups, point-in-time recovery |
| **Access control** | Kubernetes RBAC integration | IAM policies, security groups |

**Key Principle:** Panoptes generates and forwards security events. Storage, retention,
and compliance certification are the responsibility of your chosen platform. This
separation of concerns ensures:

1. You benefit from cloud provider compliance investments
2. Panoptes stays focused on detection (one job, done well)
3. No custom compliance code to audit or maintain
4. Flexibility to use your organization's existing tools

### 5. Cluster & Fleet Context

**Gap:** Events contain pod/node identity but no cluster-level context. In a multi-cluster
fleet (especially Spectro Cloud Palette environments), security teams cannot determine
which cluster, region, or tenant an event belongs to without external correlation.

**Required:** Every event must include cluster identification and organizational context.

**Implementation Path:**
- Add ClusterContext message to proto definitions (argus.proto, janus.proto)
- Extend operators to capture cluster metadata from environment/downward API
- Include pod ownership (Deployment, StatefulSet, DaemonSet owner)
- Forward relevant pod labels and annotations to events
- Support Spectro Palette management cluster identification

**Why:** Multi-cluster security monitoring requires immediate identification of:
- Which cluster in the fleet was affected
- Which environment (prod vs staging) is at risk
- Which tenant/customer owns the affected workload
- Which region needs incident response

**Example Output:**
```json
{
  "event": "modify",
  "path": "/etc/passwd",
  "timestamp": "2026-01-10T14:32:00Z",
  "cluster_context": {
    "cluster_name": "prod-payment-east",
    "cluster_id": "spc-cluster-a1b2c3",
    "environment": "production",
    "region": "us-east-1",
    "zone": "us-east-1a",
    "tenant_id": "tenant-acme-corp",
    "tenant_name": "Acme Corporation",
    "management_cluster": "palette-mgmt-1"
  },
  "pod_context": {
    "owner_kind": "Deployment",
    "owner_name": "payment-api",
    "labels": {
      "app": "payment-api",
      "team": "payments",
      "tier": "backend"
    },
    "image": "gcr.io/acme/payment-api:v2.3.1"
  },
  "attribution": {
    "uid": 0,
    "process": "useradd",
    "cmdline": "useradd -m newuser"
  }
}
```

**Proto Extension:**
```protobuf
message ClusterContext {
  string cluster_name = 1;        // Human-readable cluster name
  string cluster_id = 2;          // Unique cluster identifier
  string environment = 3;         // prod, staging, dev
  string region = 4;              // Cloud region (us-east-1)
  string zone = 5;                // Availability zone
  string tenant_id = 6;           // Tenant/project identifier
  string tenant_name = 7;         // Human-readable tenant name
  string management_cluster = 8;  // Spectro Palette management cluster
}

message PodContext {
  string owner_kind = 1;          // Deployment, StatefulSet, DaemonSet, Job
  string owner_name = 2;          // Owner resource name
  map<string, string> labels = 3; // Selected pod labels
  string image = 4;               // Container image with tag
}
```

**CRD/Helm Configuration:**
```yaml
# values.yaml
global:
  cluster:
    name: "prod-payment-east"
    id: "spc-cluster-a1b2c3"
    environment: "production"
    region: "us-east-1"
    tenant:
      id: "tenant-acme-corp"
      name: "Acme Corporation"
```

---

## Priority 2: Should Have (Valuable Enhancements)

### 6. Baseline/Snapshot Management

Create "golden image" snapshots of monitored paths:
- Initial baseline capture on watcher creation
- On-demand "compare to baseline" reports
- Version history for forensic analysis
- Container image comparison (compare running container to source image)

### 7. Change Context Classification

Tag changes as authorized/unauthorized based on:
- **Time window**: Maintenance windows where changes are expected
- **Process allowlist**: Known-good processes (apt-get, yum, systemd)
- **User allowlist**: Operations team vs unknown users
- **Ticket integration**: Link to ServiceNow/JIRA change tickets

### 8. Enhanced Kubernetes Context

Building on the cluster/fleet context in Priority 1 (Section 5), enrich events with
additional workload metadata beyond the core cluster identity:
- Node labels (instance type, zone, node pool)
- Pod annotations (build info, change tickets, deployment version)
- Service account information
- Network policy context
- Resource quota and limit information

---

## Priority 3: Could Have (Future Consideration)

### 9. eBPF-Based Monitoring

As an alternative/complement to inotify:
- Falco-style syscall monitoring
- Even better process attribution
- Lower overhead at high event rates (100k+ events/sec)
- More complete audit trail

**Note:** Keep as separate component (Panoptes Syscall or similar), don't replace inotify core. Different use cases, different trade-offs.

### 10. SIEM Webhook Integration

Standard webhook export for:
- Splunk (HTTP Event Collector)
- Elastic (direct indexing)
- Datadog (OTLP)
- Azure Sentinel
- AWS Security Hub
- Chronicle

Already planned in observability section—implement when customers need it.

### 11. Stricter Proto Naming Conventions

The gRPC proto definitions currently use relaxed buf lint rules for pragmatic reasons:
- `RPC_REQUEST_RESPONSE_UNIQUE` disabled: Allows `google.protobuf.Empty` for multiple RPCs
- `RPC_RESPONSE_STANDARD_NAME` disabled: Allows semantic names like `FileEvent` instead of `StreamEventsResponse`

**Future enhancement:** Adopt stricter buf STANDARD conventions:
- Replace `google.protobuf.Empty` with explicit response types (e.g., `DestroyWatchResponse`)
- Rename response types to match RPC names:
  - `WatchState` → `GetWatchStateResponse`
  - `FileEvent` → `StreamEventsResponse`
  - `GuardState` → `GetGuardStateResponse`
  - `AccessEvent` → `StreamAccessEventsResponse`
  - `MetricsResponse` → `GetMetricsResponse`

**Trade-off:** Stricter naming improves tooling compatibility and API consistency, but requires updating C++ daemon code that references these types. Consider when doing a major version bump.

---

## What We Will NOT Build

These features are explicitly out of scope. They represent the bloat we're avoiding:

### 1. AI/ML Anomaly Detection
- Generates false positives
- Black box decision making
- Can't explain to an auditor
- Adds complexity without value

### 2. Auto-Remediation
- Dangerous in production
- Can cause outages
- Security decisions should involve humans
- Rollback without understanding is not security

### 3. Complex Policy Engines
- Thousand-rule policy libraries
- Logic that requires a PhD to understand
- Policies that no one audits

### 4. Built-in SIEM
- We're not a SIEM
- Compose with existing SIEMs
- Don't reinvent log aggregation

### 5. Fancy Data Visualizations
- 3D graphs that look cool but tell you nothing
- "Threat intelligence" heatmaps
- Animated dashboards

### 6. Risk Scores
- Made-up numbers (what does "risk score 7.3" mean?)
- Not auditable
- Creates false sense of security

### 7. Integration Marketplace
- Plugin sprawl
- Quality control nightmare
- Attack surface expansion

### 8. Multi-Tenant SaaS Features
- We're not a SaaS product
- Run it in your cluster
- Your data, your control

---

## Competitive Landscape

| Feature | Wazuh | Tripwire | CrowdStrike | OSSEC | **Panoptes** |
|---------|-------|----------|-------------|-------|--------------|
| Kubernetes-native | Partial | No | Partial | No | **Yes** |
| Open-source | Yes | No | No | Yes | **Yes** |
| Lightweight | Medium | Heavy | Medium | Light | **Light** |
| Expert-focused | Yes | No | No | Yes | **Yes** |
| Container-aware | Partial | Partial | Yes | Partial | **Yes** |
| Complexity | High | Very High | High | Medium | **Low** |
| Philosophy | HIDS platform | Enterprise GRC | XDR platform | HIDS | **Unix tool** |
| Cost | Free/$$ | $$$$ | $$$$ | Free/$$ | **Free** |

### Why Competitors Fail

**Tripwire**: Became an "enterprise GRC platform" with so many features that nobody uses it correctly. Configuration requires consultants.

**CrowdStrike FileVantage**: Part of a massive XDR suite. You're buying an aircraft carrier when you need a speedboat.

**Wazuh**: Excellent project, but trying to be everything (SIEM, HIDS, vulnerability scanner, compliance). Complexity creeps in.

**OSSEC**: Great foundation, but showing its age. Docker/rkt era, not containerd/CRI-O.

### Our Position

**Panoptes is the tool security experts choose when they know what they're doing.**

- Simple enough to audit
- Focused enough to trust
- Modern enough for Kubernetes
- Honest enough to explain every alert

---

## Implementation Roadmap

### Phase 1: Process Attribution

**Janus (fanotify) - Low Effort:**
- Extend fanotify with FAN_REPORT_PIDFD for process attribution
- Add /proc lookups for process context (cmdline, cwd, uid, gid)
- Update proto definitions and operator event processing

**Argus (inotify) - Higher Effort:**
- inotify does not provide process attribution natively
- Recommended approach: Integrate with Linux Audit subsystem for correlation
- Alternative: Add eBPF VFS hooks (higher complexity, better performance)

### Phase 2: Content Hashing (Medium Effort)
- Add hashBaseline option to CRD spec
- Implement hash computation in argusd
- Storage strategy for baselines (ConfigMap vs annotation vs external)
- Scheduled verification job
- Estimated: 1 week

### Phase 3: Compliance Reporting (UI Work)
- Report generator component in Panoptes Eye
- Framework templates (PCI-DSS, HIPAA, SOC2)
- PDF generation (use puppeteer or similar)
- Scheduled report jobs
- Estimated: 1-2 weeks

### Phase 4: Baseline Management (Future)
- Snapshot/restore architecture
- Container image comparison
- Drift detection workflows
- Estimated: 2-3 weeks

---

## Summary

Panoptes will remain a **focused, expert-grade FIM tool** that:

1. Does inotify/fanotify monitoring exceptionally well
2. Adds minimal-but-essential compliance features
3. Integrates cleanly with existing observability stacks
4. Provides auditor-friendly evidence export
5. Stays true to Unix philosophy: small, composable, transparent

**We let bloated software be bloated software.**

Our approach is tried and true, tested and obvious. When an auditor asks "how does your FIM work?", the answer is simple: "The Linux kernel tells us when files change. We log it. You can read the logs."

That's it. That's the product.

---

## Research Sources

- [CrowdStrike Falcon FileVantage](https://www.crowdstrike.com/en-us/platform/exposure-management/falcon-filevantage/)
- [Wazuh FIM Documentation](https://documentation.wazuh.com/current/getting-started/use-cases/file-integrity.html)
- [Sysdig FIM Best Practices](https://www.sysdig.com/blog/file-integrity-monitoring)
- [PCI DSS 4.0 FIM Requirements](https://blog.qualys.com/product-tech/2025/06/05/ensure-pci-4-0-readiness-with-integrated-file-monitoring-for-containers)
- [7 Regulations Requiring FIM](https://www.cimcor.com/blog/7-regulations-requiring-file-integrity-monitoring-for-compliance)
- [Unix Philosophy - Wikipedia](https://en.wikipedia.org/wiki/Unix_philosophy)
- [Complexity is the Enemy of Security - Phil Venables](https://www.philvenables.com/post/is-complexity-the-enemy-of-security)

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
