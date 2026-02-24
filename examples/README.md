# Panoptes Security Demo Scenarios

Copy-paste runnable demos for Panoptes -- kernel-level file integrity monitoring and access control for Kubernetes.

## Prerequisites

- A Kubernetes cluster (Kind works great for evaluation)
- `kubectl` configured and connected
- `helm` v3.x installed
- Panoptes installed (each demo installs it if not present)

## Scenarios

| # | Scenario | What It Demonstrates | Difficulty |
|---|----------|---------------------|------------|
| 1 | [PCI-DSS in 3 Commands](pci-dss-in-3-commands/) | PCI-DSS compliance monitoring for payment workloads with ArgusWatcher + JanusGuard | Beginner |
| 2 | [Detect Container Breakout](detect-container-breakout/) | Detection of container breakout indicators mapped to MITRE ATT&CK | Intermediate |
| 3 | [Runtime File Integrity Alert](runtime-file-integrity-alert/) | Basic FIM on nginx -- detect config and content tampering | Beginner |
| 4 | [HIPAA ePHI Protection](hipaa-ephi-protection/) | HIPAA audit controls and data integrity monitoring for healthcare workloads | Intermediate |
| 5 | [Credential Theft Detection](credential-theft-detection/) | Detect and block credential access attempts with ArgusWatcher + JanusGuard | Intermediate |

## How to Run

Every scenario includes a `demo.sh` script that is fully automated:

```bash
# Run a demo
cd examples/<scenario>/
./demo.sh

# Clean up when done
./demo.sh --cleanup
```

All scripts are idempotent -- you can run them multiple times safely.

## Environment Notes

- Demos are designed and tested on Kind clusters
- Each demo creates resources in the `default` namespace (workloads) and `panoptes-system` namespace (Panoptes itself)
- Cleanup flags remove all resources the demo created
- No cluster-admin privileges are required beyond what Panoptes needs

## Related Documentation

- [Quick Start Security Guide](../docs/guides/quick-start-security.md)
- [What to Monitor](../docs/guides/what-to-monitor.md)
- [Compliance Monitoring](../docs/guides/use-cases/compliance-monitoring.md)
- [Monitoring by Compliance Framework](../docs/guides/monitoring-by-compliance.md)
