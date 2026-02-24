# Panoptes Documentation

> Navigate by what you're trying to accomplish, not what component you're looking for.

Panoptes is a Kubernetes-native file integrity monitoring (FIM) and file access auditing system:
- **Argus**: File integrity monitoring using inotify (ArgusWatcher CRD, short name: `aw`)
- **Janus**: File access auditing and enforcement using fanotify (JanusGuard CRD, short name: `jg`)
- **Panoptes Eye**: Real-time web dashboard (Next.js)

## Getting Started

Start here if you're new to Panoptes.

- [**Dev Container Setup**](../hack/README.md#dev-container-recommended) - One-click dev environment (Docker or Podman)
- [**Quick Start: Local Testing**](QUICK_START.md) - Deploy Panoptes on a local Kind cluster
- [**Quick Start: Spectro Cloud**](SPECTRO_QUICK_START.md) - Deploy via Spectro Cloud Palette packs
- [**Webhook Setup**](WEBHOOK_DEPLOYMENT.md) - Configure conversion webhooks for CRD versioning

## Use-Case Quickstarts

Problem-first guides to get results fast.

- [**Compliance Onboarding**](guides/quickstart-compliance.md) - Meet PCI-DSS, HIPAA, SOC2 audit requirements
- [**Security Incident Detection**](guides/quickstart-incident-detection.md) - Detect container breakouts, persistence mechanisms, tampering
- [**Platform Hardening**](guides/quickstart-platform-hardening.md) - Enforce CIS-K8s benchmarks and detect config drift

## Guides

Deep dives into specific topics and workflows.

- [**What to Monitor**](guides/what-to-monitor.md) - Recommended paths, exclusions, and monitoring strategies
- [**Monitoring by Compliance Framework**](guides/monitoring-by-compliance.md) - Framework-specific path mappings (PCI-DSS, HIPAA, SOC2, CIS)
- [**Monitoring & Alerting**](guides/monitoring-alerting.md) - Prometheus metrics, Grafana dashboards, alert rules
- [**Enabling Enforcement**](guides/enabling-enforcement.md) - Block unauthorized file access with Janus policies
- [**Multi-Cluster Monitoring**](guides/multi-cluster.md) - Central aggregation and cross-cluster correlation
- [**5-Minute Security Setup**](guides/quick-start-security.md) - Essential security configurations for production
- [**Practical Use Cases**](guides/use-cases/README.md) - Real-world scenarios and example configurations
- [**Troubleshooting**](guides/troubleshooting.md) - Common issues and diagnostic procedures

## Platform Guides

Platform-specific deployment instructions and optimizations.

- [**Google Kubernetes Engine (GKE)**](guides/platforms/gke.md) - GKE deployment with Container-Optimized OS considerations
- [**Amazon Elastic Kubernetes Service (EKS)**](guides/platforms/eks.md) - EKS deployment with Bottlerocket and AL2 support
- [**Azure Kubernetes Service (AKS)**](guides/platforms/aks.md) - AKS deployment with Ubuntu and Mariner node configurations
- [**Kind (Local Development)**](guides/platforms/kind.md) - Local testing with Kind clusters

## Security

Threat analysis, hardening guides, and security guarantees.

- [**Threat Model**](security/threat-model.md) - Attack vectors, trust boundaries, and mitigations
- [**Attack Surface Analysis**](security/attack-surface-analysis.md) - Exposed interfaces and risk assessment
- [**Advanced Hardening**](security/advanced-hardening.md) - Production-grade security configurations
- [**Privileged Container Justification**](security/privileged-container-justification.md) - Why daemons need elevated capabilities
- [**Remediation Plan**](security/remediation-plan.md) - Security findings and mitigation roadmap
- [**Rust Security Practices**](security/rust-security-practices.md) - Memory safety and secure coding patterns
- [**Cryptographic Guarantees**](security/cryptographic-guarantees.md) - Hash algorithms and integrity verification
- [**Vulnerability Response**](security/vulnerability-response.md) - Security disclosure and patching process

## Operations

Performance tuning, observability, and operational best practices.

- [**Kernel Tuning**](operations/kernel-tuning.md) - Optimize inotify/fanotify limits for large-scale deployments

## API Reference

Auto-generated CRD documentation from Go source code.

- [**ArgusWatcher v2 API**](api/arguswatcher-v2.md) - Complete spec, status, and field reference
- [**JanusGuard v2 API**](api/janusguard-v2.md) - Complete spec, status, and field reference

## Architecture & Reference

Design decisions and future roadmap.

- [**Future State & Roadmap**](FUTURE_STATE.md) - Planned features and architectural evolution

---

**Quick Reference:**
- ArgusWatcher short name: `aw` (e.g., `kubectl get aw`)
- JanusGuard short name: `jg` (e.g., `kubectl get jg`)
- Required capabilities:
  - argusd: `SYS_PTRACE` (required), `DAC_READ_SEARCH` (optional)
  - janusd: `SYS_ADMIN`, `SYS_PTRACE` (required), `AUDIT_WRITE` (optional)
- Daemon ports: argusd (50051), janusd (50052)
- Operator metrics: port 8080
