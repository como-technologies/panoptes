#!/bin/bash
#
# Panoptes Compliance Assessment Tool
#
# Analyzes a Kubernetes cluster and recommends appropriate Panoptes
# compliance templates based on workload characteristics.
#
# Usage: ./assess.sh [OPTIONS]
#
# Options:
#   --output=FORMAT    Output format: report (default), json, yaml
#   --namespace=NS     Scan specific namespace (default: all)
#   --exclude-ns=NS    Exclude namespace(s), comma-separated
#   --verbose          Show detailed output
#   --help             Show this help message
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source library functions
source "${SCRIPT_DIR}/lib/detect-workloads.sh"
source "${SCRIPT_DIR}/lib/detect-labels.sh"
source "${SCRIPT_DIR}/lib/gap-analysis.sh"
source "${SCRIPT_DIR}/lib/output.sh"

# Defaults
OUTPUT_FORMAT="report"
TARGET_NAMESPACE=""
EXCLUDE_NAMESPACES="kube-system,kube-public,kube-node-lease,panoptes-system"
VERBOSE=false

# Colors (disabled if not a terminal)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    NC='\033[0m' # No Color
else
    RED='' GREEN='' YELLOW='' BLUE='' NC=''
fi

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --output=*)
                OUTPUT_FORMAT="${1#*=}"
                ;;
            --namespace=*)
                TARGET_NAMESPACE="${1#*=}"
                ;;
            --exclude-ns=*)
                EXCLUDE_NAMESPACES="${1#*=}"
                ;;
            --verbose)
                VERBOSE=true
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                echo "Unknown option: $1" >&2
                show_help
                exit 1
                ;;
        esac
        shift
    done
}

show_help() {
    cat << 'EOF'
Panoptes Compliance Assessment Tool

Analyzes a Kubernetes cluster and recommends appropriate Panoptes
compliance templates based on workload characteristics.

USAGE:
    ./assess.sh [OPTIONS]

OPTIONS:
    --output=FORMAT    Output format: report (default), json, yaml
    --namespace=NS     Scan specific namespace (default: all)
    --exclude-ns=NS    Exclude namespace(s), comma-separated
                       (default: kube-system,kube-public,kube-node-lease)
    --verbose          Show detailed output
    --help             Show this help message

EXAMPLES:
    # Run assessment with default settings
    ./assess.sh

    # Output JSON for automation
    ./assess.sh --output=json

    # Scan only production namespace
    ./assess.sh --namespace=production

    # Generate YAML manifests for labeling
    ./assess.sh --output=yaml > labels.yaml

OUTPUT FORMATS:
    report    Markdown report with findings and recommendations
    json      Machine-readable JSON for CI/CD integration
    yaml      Ready-to-apply Kubernetes manifests for labeling

DETECTION RULES:
    The tool detects workloads that may require compliance monitoring:

    PCI-DSS:  Payment, stripe, checkout, card, billing keywords
    HIPAA:    Health, patient, medical, ehr, fhir, phi keywords
    GDPR:     Privacy, consent, pii, personal-data, eu- keywords
    SOC 2:    SaaS, customer-data, audit keywords
    CIS K8s:  Privileged containers, hostPID, hostNetwork, runtime sockets

EOF
}

# Check prerequisites
check_prerequisites() {
    if ! command -v kubectl &> /dev/null; then
        echo "Error: kubectl is not installed or not in PATH" >&2
        exit 1
    fi

    if ! kubectl cluster-info &> /dev/null; then
        echo "Error: Cannot connect to Kubernetes cluster" >&2
        exit 1
    fi
}

# Progress indicator (always shown on stderr, doesn't interfere with json/yaml on stdout)
show_progress() {
    local msg="$1"
    echo -e "${BLUE}[*]${NC} ${msg}..." >&2
}

# Main assessment function
run_assessment() {
    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    show_progress "Connecting to cluster"

    # Get cluster info
    local cluster_name
    cluster_name=$(kubectl config current-context 2>/dev/null || echo "unknown")

    # Get namespaces to scan
    local namespaces
    if [[ -n "$TARGET_NAMESPACE" ]]; then
        namespaces="$TARGET_NAMESPACE"
    else
        show_progress "Discovering namespaces"
        namespaces=$(get_namespaces "$EXCLUDE_NAMESPACES")
    fi

    local ns_count
    ns_count=$(echo "$namespaces" | wc -w | tr -d ' ')
    show_progress "Found ${ns_count} namespaces to scan"

    # Run detections
    show_progress "Detecting workloads (this may take a moment)"
    local workload_findings
    workload_findings=$(detect_workloads "$namespaces")

    show_progress "Discovering compliance labels"
    local label_findings
    label_findings=$(detect_labels "$namespaces")

    show_progress "Checking existing Panoptes resources"
    local existing_resources
    existing_resources=$(detect_panoptes_resources)

    show_progress "Running gap analysis"
    local gap_analysis
    gap_analysis=$(run_gap_analysis "$workload_findings" "$label_findings" "$existing_resources")

    [[ "$OUTPUT_FORMAT" == "report" ]] && echo -e "${GREEN}[✓]${NC} Assessment complete\n" >&2

    # Generate output
    case "$OUTPUT_FORMAT" in
        report)
            output_report "$timestamp" "$cluster_name" "$namespaces" \
                "$workload_findings" "$label_findings" "$existing_resources" "$gap_analysis"
            ;;
        json)
            output_json "$timestamp" "$cluster_name" "$namespaces" \
                "$workload_findings" "$label_findings" "$existing_resources" "$gap_analysis"
            ;;
        yaml)
            output_yaml "$workload_findings" "$label_findings" "$gap_analysis"
            ;;
        *)
            echo "Unknown output format: $OUTPUT_FORMAT" >&2
            exit 1
            ;;
    esac
}

# Get namespaces to scan
get_namespaces() {
    local exclude="$1"
    local exclude_pattern
    exclude_pattern=$(echo "$exclude" | tr ',' '|')

    kubectl get namespaces -o jsonpath='{.items[*].metadata.name}' | \
        tr ' ' '\n' | \
        grep -vE "^($exclude_pattern)$" | \
        tr '\n' ' '
}

# Main
main() {
    parse_args "$@"
    check_prerequisites
    run_assessment
}

main "$@"
