# Panoptes Roadmap

This document outlines the planned development direction for Panoptes. It is updated as priorities shift based on community feedback and compliance requirements.

## Current (v2.0)

**Stable and production-ready:**

- Argus: File integrity monitoring via inotify with recursive watching, depth limits, ignore patterns
- Janus: File access auditing/blocking via fanotify with allow/deny policies and enforcement modes
- 7 compliance framework templates: PCI-DSS 4.0, HIPAA, SOC2, CIS Kubernetes, NIST 800-53, GDPR, Base Security
- Kubernetes operators with leader election, structured logging, Prometheus metrics
- Rust daemons with static musl builds (~5-8 MB), container runtime auto-detection
- Web dashboard (Panoptes Eye) with real-time event streaming
- Spectro Cloud Palette pack integration
- Helm charts with OCI registry distribution

## Next (v2.1)

**Focus: Process attribution and event export**

- [ ] **Process Attribution** — Identify WHO changed a file (UID, process name, cmdline), not just WHAT changed. Required for PCI-DSS 10.3.4 audit trails. Approach: fanotify PID metadata + /proc lookup for Janus; audit subsystem correlation for Argus.
- [ ] **Webhook Event Export** — Generic HTTP webhook sink for forwarding events to SIEMs (Splunk HEC, Elastic, Datadog). CloudEvents format support.
- [ ] **Content Hashing** — SHA-256 baseline comparison for critical files. Detect whether content actually changed vs. metadata-only changes. Required for PCI-DSS 11.5.2 and NIST SI-7(1).
- [ ] **eBPF Mode (Argus)** — Alternative to inotify using eBPF for higher-performance FIM at scale. Feature-flagged, opt-in.

## Future (v2.2+)

**Focus: Enterprise scale and multi-cluster**

- [ ] **Multi-Cluster Coordination** — Centralized policy management across clusters with fleet-wide compliance views
- [ ] **Compliance Reporting** — Export audit evidence in formats auditors expect (CSV, PDF). Map events to specific control IDs.
- [ ] **Baseline Snapshots** — Point-in-time filesystem state capture for drift detection
- [ ] **Event Retention Policies** — Configurable retention with automatic archival to object storage (S3, GCS)
- [ ] **Init Container Injection** — Webhook-based automatic injection of monitoring sidecars

## Non-Goals

These are things we deliberately choose not to build into Panoptes:

- **AI/ML-based anomaly detection** — We use kernel primitives, not heuristics
- **Auto-remediation** — Alerting is our job; remediation is yours
- **Built-in SIEM/log aggregation** — We export to your existing stack
- **Network policy enforcement** — Out of scope; use Cilium, Calico, etc.
- **Image scanning** — Out of scope; use Trivy, Grype, etc.

## Contributing

Have a feature request? Open a [GitHub Issue](https://github.com/como-technologies/panoptes/issues) with the `enhancement` label. We prioritize features that:

1. Address specific compliance framework requirements
2. Improve kernel-level detection accuracy
3. Reduce operational friction
4. Maintain the "one tool, one job" philosophy
