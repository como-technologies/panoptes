# Panoptes Suite

**Know when files change. Control who accesses them. In real-time. Across your cluster.**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-1.28%2B-blue.svg)](https://kubernetes.io)

Containers shouldn't be touching sensitive files. Production workloads shouldn't be reading credentials off disk.
When they do, you need to know immediately -- not hours later in a log aggregator.

Panoptes provides kernel-level file integrity monitoring and access auditing for Kubernetes.
It detects file modifications, controls file access, and streams events in real-time using
Linux inotify and fanotify -- the same mechanisms the kernel itself uses.

## Philosophy

> **One tool, one job, done well.**

While enterprise security platforms pile on AI/ML detection, auto-remediation, and
thousand-rule policy engines, we take the opposite approach:

**Tried. Tested. Obvious. Explainable.**

- **Kernel-level detection**: Linux inotify and fanotify -- no heuristics, no guessing
- **Kubernetes-native**: CRDs, operators, label selectors -- not YAML soup or agent sprawl
- **Transparent**: Clear logs, obvious behavior -- you can explain any alert to an auditor
- **Composable**: Works with Prometheus, Grafana, Loki, your SIEM -- we don't replace them

We explicitly avoid: AI/ML black boxes, auto-remediation, complex policy engines, risk scores.

---

## Components

| Component | Purpose | When to Use |
|-----------|---------|-------------|
| **Argus** | File Integrity Monitoring (inotify) | Detect changes to config files, binaries, certs |
| **Janus** | File Access Auditing (fanotify) | Block/audit access to sensitive files |
| **Panoptes Eye** | Web Dashboard | Visualize events, manage CRDs, browse filesystems |

### When to Use Argus vs. Janus

| Scenario | Argus | Janus |
|----------|:-----:|:-----:|
| Compliance monitoring (PCI-DSS, SOC2, HIPAA) | ✓ | ✓ |
| Detect config drift | ✓ | |
| Detect persistence mechanisms (cron, ssh keys) | ✓ | |
| Block access to sensitive files | | ✓ |
| Runtime protection / enforcement | | ✓ |
| Audit who accessed what | | ✓ |

**Use both** for defense in depth: Argus detects changes, Janus controls access.

---

## Quick Start (2 minutes)

The fastest way to try Panoptes:

```bash
# Create a Kind cluster and deploy everything
./hack/local-deploy.sh all

# Access the dashboard
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
# Open http://localhost:3000
```

**Other deployment options:**
- [Production (Kustomize)](docs/QUICK_START.md) -- `kubectl apply -k deploy/`
- [Spectro Cloud Palette](docs/SPECTRO_QUICK_START.md) -- Pack-based deployment
- [Platform guides](docs/guides/platforms/) -- EKS, GKE, AKS-specific instructions

---

## Your First Policies

### Monitor Critical Files (Argus)

```yaml
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: critical-files
spec:
  selector:
    matchLabels:
      app: my-app
  subjects:
    - paths: [/etc/passwd, /etc/shadow, /etc/sudoers]
      events: [modify, delete, attrib]
```

```bash
kubectl apply -f watcher.yaml
kubectl get aw  # Short name: aw
```

### Block Sensitive Access (Janus)

```yaml
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: block-secrets
spec:
  selector:
    matchLabels:
      app: my-app
  enforcing: true
  subjects:
    - deny: [/etc/shadow, /root/.ssh/**]
      allow: [/app/**, /tmp/**]
      events: [open, access]
      defaultResponse: deny
```

```bash
kubectl apply -f guard.yaml
kubectl get jg  # Short name: jg
```

---

## Architecture

```mermaid
flowchart TB
    subgraph K8s["Kubernetes Cluster"]
        AW["ArgusWatcher CRD"] --> AO["Argus Operator"]
        JG["JanusGuard CRD"] --> JO["Janus Operator"]

        AO -->|"gRPC"| AD["argusd<br/>(DaemonSet)"]
        JO -->|"gRPC"| JD["janusd<br/>(DaemonSet)"]

        PE["Panoptes Eye<br/>Dashboard"]
    end

    AD -->|"inotify"| K["Linux Kernel"]
    JD -->|"fanotify"| K
```

**How it works:**
1. You create an `ArgusWatcher` or `JanusGuard` CR with label selectors
2. The operator finds matching pods and sends configs to node daemons via gRPC
3. Daemons register kernel watches on container filesystems via `/proc/{pid}/root`
4. Events stream back in real-time to the operator (and optionally to your dashboard/SIEM)

---

## What's Next?

<table>
<tr>
<td width="50%" valign="top">

### For Platform Operators

- [**Deploy to production**](docs/QUICK_START.md) -- Complete setup guide
- [**Kernel tuning**](docs/operations/kernel-tuning.md) -- inotify/fanotify limits
- [**Monitoring & alerting**](docs/guides/monitoring-alerting.md) -- Prometheus, Grafana
- [**Troubleshooting**](docs/guides/troubleshooting.md) -- Common issues

</td>
<td width="50%" valign="top">

### For Security Teams

- [**Compliance monitoring**](docs/guides/quickstart-compliance.md) -- PCI-DSS, HIPAA, SOC2
- [**Incident detection**](docs/guides/quickstart-incident-detection.md) -- Container breakouts
- [**What to monitor**](docs/guides/what-to-monitor.md) -- Recommended paths
- [**Threat model**](docs/security/threat-model.md) -- Attack vectors, mitigations

</td>
</tr>
<tr>
<td valign="top">

### For Application Owners

- [**API reference**](docs/api/) -- ArgusWatcher & JanusGuard specs
- [**Use case examples**](docs/guides/use-cases/) -- Real-world scenarios
- [**Platform hardening**](docs/guides/quickstart-platform-hardening.md) -- CIS benchmarks

</td>
<td valign="top">

### For Contributors

- [**Building from source**](docs/QUICK_START.md) -- Operators (Go), Daemons (Rust)
- [**Architecture docs**](docs/FUTURE_STATE.md) -- Design decisions, roadmap
- [**Security practices**](docs/security/rust-security-practices.md) -- Memory safety

</td>
</tr>
</table>

---

## Repository Structure

```
panoptes/
├── deploy/              # Production manifests (kubectl apply -k deploy/)
├── docs/                # Comprehensive documentation
│   ├── guides/          # Use-case guides, platform guides
│   ├── security/        # Threat model, hardening
│   └── api/             # CRD API reference
├── operators/           # Kubernetes operators (Go)
│   ├── argus-operator/  # File integrity monitoring
│   └── janus-operator/  # File access auditing
├── daemons/             # Node daemons (Rust)
│   ├── argusd/          # inotify-based FIM
│   ├── janusd/          # fanotify-based audit
│   └── common/          # Shared utilities
├── ui/panoptes-eye/     # Web dashboard (Next.js)
├── proto/               # gRPC definitions
├── packs/               # Spectro Cloud Palette packs
└── hack/                # Development scripts
```

---

## Quick Reference

| Resource | Short Name | Daemon Port | API Group |
|----------|------------|-------------|-----------|
| ArgusWatcher | `aw` | 50051 | argus.como-technologies.io |
| JanusGuard | `jg` | 50052 | janus.como-technologies.io |

**Required kernel capabilities:**
- argusd: `SYS_PTRACE` (required), `DAC_READ_SEARCH` (optional)
- janusd: `SYS_ADMIN`, `SYS_PTRACE` (required), `AUDIT_WRITE` (optional)

**Requirements:** Kubernetes 1.28+, Linux kernel 5.x+, containerd or CRI-O

---

## Community

- **Issues**: [GitHub Issues](https://github.com/como-technologies/panoptes/issues)
- **Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md)
- **Security**: Report vulnerabilities via [security policy](docs/security/vulnerability-response.md)

---

## License

Copyright 2026 Como Technologies, LTD. Licensed under [Apache 2.0](LICENSE).
