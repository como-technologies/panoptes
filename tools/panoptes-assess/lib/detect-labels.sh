#!/bin/bash
#
# Label Discovery Functions
# Discovers existing compliance labels and suggests new ones
#

# Known compliance labels
declare -a COMPLIANCE_LABELS=(
    "pci-dss/scope"
    "hipaa/scope"
    "soc2/scope"
    "gdpr/scope"
    "nist-800-53/scope"
    "cis/scope"
    "panoptes.como-technologies.io/monitored"
)

# Expected label values
declare -A LABEL_VALUES
LABEL_VALUES[pci-dss/scope]="in-scope"
LABEL_VALUES[hipaa/scope]="ephi"
LABEL_VALUES[soc2/scope]="in-scope"
LABEL_VALUES[gdpr/scope]="personal-data"
LABEL_VALUES[nist-800-53/scope]="moderate"
LABEL_VALUES[cis/scope]="kubernetes-audit"
LABEL_VALUES[panoptes.como-technologies.io/monitored]="true"

# Discover existing compliance labels
detect_labels() {
    local namespaces="$1"
    local findings=""

    for ns in $namespaces; do
        # Get all pods with their labels
        local pods_json
        pods_json=$(kubectl get pods -n "$ns" -o json 2>/dev/null || echo '{"items":[]}')

        while IFS= read -r pod_info; do
            [[ -z "$pod_info" ]] && continue

            local pod_name ns_name
            pod_name=$(echo "$pod_info" | cut -d'|' -f1)
            ns_name=$(echo "$pod_info" | cut -d'|' -f2)

            local found_labels=""

            # Check for each compliance label
            for label in "${COMPLIANCE_LABELS[@]}"; do
                local label_value
                # Handle labels with slashes (need to escape for jsonpath)
                local escaped_label="${label//\//\\/}"
                label_value=$(kubectl get pod "$pod_name" -n "$ns_name" \
                    -o jsonpath="{.metadata.labels['${escaped_label}']}" 2>/dev/null || echo "")

                if [[ -n "$label_value" ]]; then
                    found_labels="${found_labels}${label}=${label_value},"
                fi
            done

            if [[ -n "$found_labels" ]]; then
                found_labels="${found_labels%,}"  # Remove trailing comma
                findings="${findings}${pod_name}|${ns_name}|labeled|${found_labels}\n"
            else
                findings="${findings}${pod_name}|${ns_name}|unlabeled|\n"
            fi

        done < <(echo "$pods_json" | jq -r '.items[] | "\(.metadata.name)|\(.metadata.namespace)"' 2>/dev/null)
    done

    echo -e "$findings"
}

# Get pods with specific compliance label
get_labeled_pods() {
    local label="$1"
    local namespaces="$2"
    local results=""

    for ns in $namespaces; do
        local pods
        pods=$(kubectl get pods -n "$ns" -l "$label" -o jsonpath='{.items[*].metadata.name}' 2>/dev/null || echo "")
        if [[ -n "$pods" ]]; then
            for pod in $pods; do
                results="${results}${pod}|${ns}\n"
            done
        fi
    done

    echo -e "$results"
}

# Count pods by label status
count_labeled_pods() {
    local namespaces="$1"
    local label="$2"

    local count=0
    for ns in $namespaces; do
        local ns_count
        ns_count=$(kubectl get pods -n "$ns" -l "$label" --no-headers 2>/dev/null | wc -l || echo 0)
        count=$((count + ns_count))
    done

    echo "$count"
}

# Get unlabeled pods that should probably be labeled
suggest_labels() {
    local workload_findings="$1"
    local label_findings="$2"
    local suggestions=""

    # Parse workload findings and suggest labels
    while IFS='|' read -r pod_name ns_name frameworks risk_level reasons; do
        [[ -z "$pod_name" ]] && continue

        # Check if pod is already labeled for these frameworks
        local pod_labels
        pod_labels=$(echo -e "$label_findings" | grep "^${pod_name}|${ns_name}|" | cut -d'|' -f4)

        for framework in $(echo "$frameworks" | tr ',' ' '); do
            local label_key=""
            local label_value=""

            case "$framework" in
                pci-dss)
                    label_key="pci-dss/scope"
                    label_value="in-scope"
                    ;;
                hipaa)
                    label_key="hipaa/scope"
                    label_value="ephi"
                    ;;
                gdpr)
                    label_key="gdpr/scope"
                    label_value="personal-data"
                    ;;
                soc2)
                    label_key="soc2/scope"
                    label_value="in-scope"
                    ;;
                nist-800-53)
                    label_key="nist-800-53/scope"
                    label_value="moderate"
                    ;;
                cis-kubernetes)
                    label_key="cis/scope"
                    label_value="kubernetes-audit"
                    ;;
                base-security)
                    label_key="panoptes.como-technologies.io/monitored"
                    label_value="true"
                    ;;
            esac

            # Check if already labeled
            if [[ -n "$label_key" ]] && ! echo "$pod_labels" | grep -q "$label_key"; then
                suggestions="${suggestions}${pod_name}|${ns_name}|${label_key}|${label_value}|${reasons}\n"
            fi
        done

    done < <(echo -e "$workload_findings")

    echo -e "$suggestions"
}
