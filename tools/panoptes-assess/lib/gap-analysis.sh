#!/bin/bash
#
# Gap Analysis Functions
# Analyzes compliance coverage gaps
#

# Detect existing Panoptes resources
detect_panoptes_resources() {
    local findings=""

    # Check for ArgusWatchers
    local arguswatchers
    arguswatchers=$(kubectl get arguswatchers -A -o json 2>/dev/null || echo '{"items":[]}')

    while IFS= read -r aw_info; do
        [[ -z "$aw_info" ]] && continue
        findings="${findings}arguswatcher|${aw_info}\n"
    done < <(echo "$arguswatchers" | jq -r '.items[] | "\(.metadata.name)|\(.metadata.namespace)|\(.metadata.labels // {} | to_entries | map("\(.key)=\(.value)") | join(","))"' 2>/dev/null)

    # Check for JanusGuards
    local janusguards
    janusguards=$(kubectl get janusguards -A -o json 2>/dev/null || echo '{"items":[]}')

    while IFS= read -r jg_info; do
        [[ -z "$jg_info" ]] && continue
        findings="${findings}janusguard|${jg_info}\n"
    done < <(echo "$janusguards" | jq -r '.items[] | "\(.metadata.name)|\(.metadata.namespace)|\(.metadata.labels // {} | to_entries | map("\(.key)=\(.value)") | join(","))|\(.spec.enforcing // false)"' 2>/dev/null)

    echo -e "$findings"
}

# Run gap analysis
run_gap_analysis() {
    local workload_findings="$1"
    local label_findings="$2"
    local existing_resources="$3"
    local analysis=""

    # Frameworks to check
    local frameworks=("pci-dss" "hipaa" "soc2" "gdpr" "nist-800-53" "cis-kubernetes" "base-security")

    for framework in "${frameworks[@]}"; do
        local detected_count=0
        local labeled_count=0
        local monitored_count=0
        local enforcing=false

        # Count detected workloads for this framework (ensure numeric)
        detected_count=$(echo -e "$workload_findings" | grep -c "${framework}" 2>/dev/null | tr -d '[:space:]')
        [[ -z "$detected_count" || ! "$detected_count" =~ ^[0-9]+$ ]] && detected_count=0

        # Count labeled workloads
        case "$framework" in
            pci-dss)
                labeled_count=$(echo -e "$label_findings" | grep -c "pci-dss/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            hipaa)
                labeled_count=$(echo -e "$label_findings" | grep -c "hipaa/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            soc2)
                labeled_count=$(echo -e "$label_findings" | grep -c "soc2/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            gdpr)
                labeled_count=$(echo -e "$label_findings" | grep -c "gdpr/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            nist-800-53)
                labeled_count=$(echo -e "$label_findings" | grep -c "nist-800-53/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            cis-kubernetes)
                labeled_count=$(echo -e "$label_findings" | grep -c "cis/scope" 2>/dev/null | tr -d '[:space:]')
                ;;
            base-security)
                labeled_count=$(echo -e "$label_findings" | grep -c "panoptes.como-technologies.io/monitored" 2>/dev/null | tr -d '[:space:]')
                ;;
        esac
        [[ -z "$labeled_count" || ! "$labeled_count" =~ ^[0-9]+$ ]] && labeled_count=0

        # Check if Panoptes resources exist for this framework
        local has_arguswatcher=false
        local has_janusguard=false

        if echo -e "$existing_resources" | grep -q "compliance=${framework}"; then
            if echo -e "$existing_resources" | grep "arguswatcher" | grep -q "compliance=${framework}"; then
                has_arguswatcher=true
            fi
            if echo -e "$existing_resources" | grep "janusguard" | grep -q "compliance=${framework}"; then
                has_janusguard=true
                # Check if enforcing
                if echo -e "$existing_resources" | grep "janusguard" | grep "compliance=${framework}" | grep -q "true$"; then
                    enforcing=true
                fi
            fi
        fi

        # Determine gap and priority
        local gap=$((detected_count - labeled_count))
        [[ $gap -lt 0 ]] && gap=0

        local priority="none"
        local recommendation=""

        if [[ $detected_count -gt 0 ]]; then
            if [[ "$has_arguswatcher" == false ]] || [[ "$has_janusguard" == false ]]; then
                priority="high"
                recommendation="Deploy ${framework} template"
            elif [[ $gap -gt 0 ]]; then
                priority="medium"
                recommendation="Label ${gap} detected workloads"
            elif [[ "$enforcing" == false ]] && [[ "$framework" != "base-security" ]]; then
                priority="low"
                recommendation="Consider enabling enforcement"
            fi
        fi

        analysis="${analysis}${framework}|${detected_count}|${labeled_count}|${has_arguswatcher}|${has_janusguard}|${enforcing}|${gap}|${priority}|${recommendation}\n"
    done

    echo -e "$analysis"
}

# Get priority-sorted recommendations
get_recommendations() {
    local gap_analysis="$1"
    local workload_findings="$2"
    local recommendations=""

    # Sort by priority (high > medium > low)
    local high_priority
    high_priority=$(echo -e "$gap_analysis" | grep "|high|")

    local medium_priority
    medium_priority=$(echo -e "$gap_analysis" | grep "|medium|")

    local low_priority
    low_priority=$(echo -e "$gap_analysis" | grep "|low|")

    # Format high priority recommendations
    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue

        local details=""
        # Get specific workloads
        details=$(echo -e "$workload_findings" | grep "${framework}" | head -5)

        recommendations="${recommendations}HIGH|${framework}|${recommendation}|${details}\n"
    done < <(echo -e "$high_priority")

    # Format medium priority
    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        recommendations="${recommendations}MEDIUM|${framework}|${recommendation}|\n"
    done < <(echo -e "$medium_priority")

    # Format low priority
    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        recommendations="${recommendations}LOW|${framework}|${recommendation}|\n"
    done < <(echo -e "$low_priority")

    echo -e "$recommendations"
}
