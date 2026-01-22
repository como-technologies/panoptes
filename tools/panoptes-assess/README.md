# Panoptes Compliance Assessment Tool

Analyzes a Kubernetes cluster and recommends appropriate Panoptes compliance templates based on workload characteristics.

## Quick Start

```bash
# Run assessment with default settings (outputs markdown report)
./assess.sh

# Output JSON for automation
./assess.sh --output=json

# Generate YAML manifests for labeling workloads
./assess.sh --output=yaml > labels.yaml

# Scan specific namespace
./assess.sh --namespace=production
```

## Prerequisites

- `kubectl` configured with cluster access
- `jq` (optional, for better JSON parsing)
- Bash 4.0+

## Usage

```
./assess.sh [OPTIONS]

Options:
    --output=FORMAT    Output format: report (default), json, yaml
    --namespace=NS     Scan specific namespace (default: all)
    --exclude-ns=NS    Exclude namespace(s), comma-separated
                       (default: kube-system,kube-public,kube-node-lease)
    --verbose          Show detailed output
    --help             Show help message
```

## Output Formats

### Report (default)

Markdown report with:
- Executive summary table
- Priority-sorted recommendations
- Ready-to-run kubectl commands
- Detailed findings

```bash
./assess.sh --output=report
```

### JSON

Machine-readable format for CI/CD integration:

```bash
./assess.sh --output=json | jq '.assessment.recommendations'
```

### YAML

Ready-to-apply Kubernetes manifests for labeling workloads:

```bash
./assess.sh --output=yaml > labels.yaml
kubectl apply -f labels.yaml  # Review first!
```

## Detection Rules

The tool detects workloads that may require compliance monitoring:

| Framework | Detection Patterns |
|-----------|-------------------|
| **PCI-DSS** | payment, stripe, checkout, card, billing, transaction |
| **HIPAA** | health, patient, medical, ehr, fhir, phi, clinical |
| **GDPR** | gdpr, privacy, consent, pii, personal-data, eu- |
| **SOC 2** | soc2, saas, customer-data, audit-log |
| **NIST 800-53** | nist, fedramp, fisma, federal, government |
| **CIS Kubernetes** | privileged containers, hostPID, hostNetwork, runtime sockets |
| **Base Security** | Database workloads (postgres, mysql, mongodb, redis) |

## Example Output

```markdown
# Panoptes Compliance Assessment Report

## Executive Summary

| Framework | Detected | Labeled | ArgusWatcher | JanusGuard | Gap |
|-----------|----------|---------|--------------|------------|-----|
| pci-dss   | 5        | 0       | ❌           | ❌         | 5   |
| hipaa     | 2        | 2       | ✅           | ✅         | 0   |

## Recommendations

### 🔴 HIGH Priority

**pci-dss**: Deploy pci-dss template

Detected workloads:
  - payment-api (namespace: prod) - Matches pci-dss pattern

Commands:
kubectl label pod payment-api -n prod pci-dss/scope=in-scope
kubectl apply -f deploy/compliance/pci-dss/template.yaml
```

## Running as Kubernetes Job

See `deploy/assessment/` for Kubernetes Job manifests:

```bash
kubectl apply -f deploy/assessment/rbac.yaml
kubectl apply -f deploy/assessment/job.yaml
kubectl logs -f job/panoptes-assess -n panoptes-system
```

## File Structure

```
tools/panoptes-assess/
├── assess.sh                 # Main script
├── lib/
│   ├── detect-workloads.sh   # Workload pattern detection
│   ├── detect-labels.sh      # Compliance label discovery
│   ├── gap-analysis.sh       # Framework gap analysis
│   └── output.sh             # Output formatting
└── README.md
```

## Compliance Labels Reference

| Framework | Label | Value |
|-----------|-------|-------|
| PCI-DSS | `pci-dss/scope` | `in-scope` |
| HIPAA | `hipaa/scope` | `ephi` |
| SOC 2 | `soc2/scope` | `in-scope` |
| GDPR | `gdpr/scope` | `personal-data` |
| NIST 800-53 | `nist-800-53/scope` | `moderate` |
| CIS Kubernetes | `cis/scope` | `kubernetes-audit` |
| Base Security | `panoptes.como-technologies.io/monitored` | `true` |

## Contributing

To add new detection patterns:

1. Edit `lib/detect-workloads.sh`
2. Add pattern to `FRAMEWORK_PATTERNS` associative array
3. Update this README

## License

Apache 2.0 - See repository LICENSE file.
