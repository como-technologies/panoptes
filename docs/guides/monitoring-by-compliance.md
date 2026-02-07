# Monitoring by Compliance Framework

This guide maps common compliance requirements to specific Panoptes monitoring configurations.

## PCI-DSS (Payment Card Industry Data Security Standard)

PCI-DSS requires file integrity monitoring (FIM) and access controls for systems that store, process, or transmit cardholder data.

### Key Requirements

#### 10.5.5 - File Integrity Monitoring on Logs

> "Use file integrity monitoring or change-detection software on logs to ensure that existing log data cannot be changed without generating alerts."

**Panoptes Configuration:**
```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
spec:
  subjects:
    - paths:
        - /var/log
      events:
        - modify
        - delete
      recursive: true
      ignore:
        - "*.gz"      # Rotated logs
        - "*.old"
      tags:
        requirement: "10.5.5"
```

#### 11.5 - Change Detection on Critical Files

> "Deploy a change-detection mechanism to alert personnel to unauthorized modification of critical system files, configuration files, or content files."

**Panoptes Configuration:**
```yaml
subjects:
  # Critical system files
  - paths:
      - /etc/passwd
      - /etc/shadow
      - /etc/sudoers
    events: [modify, delete, attrib]
    tags:
      requirement: "11.5"
      severity: critical

  # Configuration files
  - paths:
      - /etc
    events: [modify, create, delete]
    recursive: true
    maxDepth: 2
```

#### 7.1 - Access Control

> "Limit access to system components and cardholder data to only those individuals whose job requires such access."

**Panoptes Configuration:**
```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  subjects:
    - deny:
        - /etc/shadow
        - /root/.ssh
      events: [access, open]
      defaultResponse: deny
      audit: true
      tags:
        requirement: "7.1"
```

### PCI-DSS Template

Use the pre-built template: [`deploy/compliance/pci-dss/template.yaml`](../../deploy/compliance/pci-dss/template.yaml)

---

## HIPAA (Health Insurance Portability and Accountability Act)

HIPAA Security Rule requires audit controls and integrity mechanisms for systems containing electronic Protected Health Information (ePHI).

### Key Requirements

#### 164.312(b) - Audit Controls

> "Implement hardware, software, and/or procedural mechanisms that record and examine activity in information systems that contain or use electronic protected health information."

**Panoptes Configuration:**
```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  subjects:
    # Audit all access to ePHI directories
    - allow:
        - /app/data
        - /var/lib/app/patient-data
      events: [access, open, modify]
      audit: true
      tags:
        requirement: "164.312(b)"
```

#### 164.312(c)(1) - Data Integrity

> "Implement policies and procedures to protect electronic protected health information from improper alteration or destruction."

**Panoptes Configuration:**
```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
spec:
  subjects:
    - paths:
        - /var/log
        - /app/audit
      events: [modify, delete]
      recursive: true
      tags:
        requirement: "164.312(c)(1)"
```

#### 164.312(d) - Person or Entity Authentication

> "Implement procedures to verify that a person or entity seeking access to electronic protected health information is the one claimed."

**Panoptes Configuration:**
```yaml
subjects:
  # Monitor authentication configuration
  - paths:
      - /etc/pam.d
      - /etc/nsswitch.conf
    events: [modify, create, delete]
    tags:
      requirement: "164.312(d)"

  # Protect credential files
  - deny:
      - /etc/shadow
      - /root/.ssh
    events: [access, open]
    defaultResponse: deny
```

### HIPAA Template

Use the pre-built template: [`deploy/compliance/hipaa/template.yaml`](../../deploy/compliance/hipaa/template.yaml)

---

## SOC 2 (Service Organization Control 2)

SOC 2 Trust Services Criteria focuses on security, availability, processing integrity, confidentiality, and privacy.

### Key Criteria

#### CC6.1 - Logical Access Security

> "The entity implements logical access security software, infrastructure, and architectures over protected information assets."

**Panoptes Configuration:**
```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  subjects:
    - deny:
        - /etc/shadow
        - /root/.ssh/id_*
      events: [access, open]
      defaultResponse: deny
      audit: true
      tags:
        criteria: "CC6.1"
```

#### CC7.2 - System Monitoring

> "The entity monitors system components and the operation of those components for anomalies that are indicative of malicious acts, natural disasters, and errors."

**Panoptes Configuration:**
```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
spec:
  subjects:
    # System binary changes
    - paths:
        - /usr/bin
        - /usr/sbin
      events: [modify, create]
      tags:
        criteria: "CC7.2"
        severity: high

    # Log integrity
    - paths:
        - /var/log
      events: [delete, modify]
```

#### CC7.3 - Incident Detection

> "The entity evaluates security events to determine whether they could or have resulted in a failure of the entity to meet its objectives."

**Panoptes Configuration:**
```yaml
subjects:
  # Audit suspicious tool usage
  - deny:
      - /usr/bin/curl
      - /usr/bin/wget
      - /usr/bin/nc
    events: [execute]
    defaultResponse: audit
    audit: true
    tags:
      criteria: "CC7.3"
```

### SOC 2 Template

Use the pre-built template: [`deploy/compliance/soc2/template.yaml`](../../deploy/compliance/soc2/template.yaml)

---

## CIS Kubernetes Benchmark

The CIS Kubernetes Benchmark provides security configuration guidelines for Kubernetes deployments.

### Key Sections

#### 1.1.x - Control Plane Configuration Files

> "Ensure API server pod specification file permissions are set to 644 or more restrictive."

**Panoptes Configuration:**
```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
spec:
  subjects:
    - paths:
        - /etc/kubernetes/manifests
        - /etc/kubernetes/pki
      events: [modify, create, delete, attrib]
      recursive: true
      tags:
        benchmark: "CIS 1.1"
```

#### 4.2.x - Kubelet Configuration

> "Ensure that the kubelet configuration file permissions are set to 644 or more restrictive."

**Panoptes Configuration:**
```yaml
subjects:
  - paths:
      - /var/lib/kubelet/config.yaml
      - /etc/kubernetes/kubelet.conf
    events: [modify, attrib]
    tags:
      benchmark: "CIS 4.2"
```

#### 5.1.6 - Service Account Tokens

> "Ensure that Service Account Tokens are only mounted where necessary."

**Panoptes Configuration:**
```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
spec:
  subjects:
    - deny:
        - /var/run/secrets/kubernetes.io/serviceaccount
      events: [access, open]
      autoAllowOwner: true
      audit: true
      tags:
        benchmark: "CIS 5.1.6"
```

### CIS Kubernetes Template

Use the pre-built template: [`deploy/compliance/cis-kubernetes/template.yaml`](../../deploy/compliance/cis-kubernetes/template.yaml)

---

## Multi-Framework Compliance

Many organizations need to comply with multiple frameworks. Panoptes supports this through:

### 1. Multiple Labels

Apply multiple labels to pods that need multi-framework compliance:

```bash
kubectl label pod payment-api \
  pci-dss/scope=in-scope \
  soc2/scope=in-scope
```

### 2. Combined Templates

Create a single ArgusWatcher/JanusGuard with subjects from multiple frameworks:

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: multi-compliance-fim
  labels:
    compliance: pci-dss,soc2
spec:
  selector:
    matchLabels:
      compliance/multi: "true"
  subjects:
    # PCI-DSS 11.5
    - paths: [/etc/passwd, /etc/shadow]
      events: [modify, delete]
      tags:
        pci-dss: "11.5"
        soc2: "CC7.2"

    # Both frameworks - log monitoring
    - paths: [/var/log]
      events: [modify, delete]
      tags:
        pci-dss: "10.5.5"
        soc2: "CC7.2"
```

### 3. Tag-Based Reporting

Use tags consistently across templates for unified reporting:

```yaml
tags:
  compliance: pci-dss
  requirement: "11.5"
  severity: critical
  category: file-integrity
```

---

## Compliance Dashboard

The Panoptes Eye dashboard (`/compliance` page) automatically evaluates your current monitoring against compliance requirements:

1. **Framework Selection**: Filter by PCI-DSS, HIPAA, SOC 2, or CIS
2. **Score Display**: Overall compliance percentage
3. **Check Details**: Pass/fail/warning status per requirement
4. **Remediation**: Guidance for failing checks

### Improving Your Score

1. Apply relevant compliance templates
2. Label all in-scope pods correctly
3. Address warning checks (partial coverage)
4. Move JanusGuards from audit to enforcing mode

---

## Related Documentation

- [Compliance Templates](../../deploy/compliance/)
- [What to Monitor](./what-to-monitor.md)
- [Quick Start Security](./quick-start-security.md)
