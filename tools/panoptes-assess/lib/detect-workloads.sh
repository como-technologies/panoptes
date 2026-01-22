#!/bin/bash
#
# Workload Detection Functions
# Detects workloads that may require compliance monitoring
#

# Detection patterns for each framework
declare -A FRAMEWORK_PATTERNS
FRAMEWORK_PATTERNS[pci-dss]="payment|stripe|checkout|card|billing|transaction|merchant|acquirer|cardholder"
FRAMEWORK_PATTERNS[hipaa]="health|patient|medical|ehr|fhir|phi|hipaa|clinical|diagnosis|prescription"
FRAMEWORK_PATTERNS[gdpr]="gdpr|privacy|consent|pii|personal-data|eu-|data-subject|dpo|right-to-forget"
FRAMEWORK_PATTERNS[soc2]="soc2|saas|customer-data|audit-log|trust-service"
FRAMEWORK_PATTERNS[nist-800-53]="nist|fedramp|fisma|federal|government|gov-"

# Database patterns (recommend base-security at minimum)
DATABASE_PATTERN="postgres|mysql|mariadb|mongodb|redis|elasticsearch|cassandra|cockroach|timescale"

# Detect workloads across namespaces
detect_workloads() {
    local namespaces="$1"
    local findings=""

    for ns in $namespaces; do
        # Get pods with their labels, images, and volume mounts in one call
        local pods_json
        pods_json=$(kubectl get pods -n "$ns" -o json 2>/dev/null || echo '{"items":[]}')

        # Process each pod - extract all needed data from JSON (no extra kubectl calls)
        while IFS= read -r pod_info; do
            [[ -z "$pod_info" ]] && continue

            local pod_name ns_name labels images host_paths privileged host_pid host_net
            pod_name=$(echo "$pod_info" | cut -d'|' -f1)
            ns_name=$(echo "$pod_info" | cut -d'|' -f2)
            labels=$(echo "$pod_info" | cut -d'|' -f3)
            images=$(echo "$pod_info" | cut -d'|' -f4)
            host_paths=$(echo "$pod_info" | cut -d'|' -f5)
            privileged=$(echo "$pod_info" | cut -d'|' -f6)
            host_pid=$(echo "$pod_info" | cut -d'|' -f7)
            host_net=$(echo "$pod_info" | cut -d'|' -f8)

            local detected_frameworks=""
            local risk_level="low"
            local reasons=""

            # Check for framework-specific patterns
            for framework in "${!FRAMEWORK_PATTERNS[@]}"; do
                local pattern="${FRAMEWORK_PATTERNS[$framework]}"
                if echo "$pod_name $labels $images" | grep -qiE "$pattern"; then
                    detected_frameworks="${detected_frameworks}${framework},"
                    risk_level="high"
                    reasons="${reasons}Matches ${framework} pattern; "
                fi
            done

            # Check for database workloads
            if echo "$images" | grep -qiE "$DATABASE_PATTERN"; then
                detected_frameworks="${detected_frameworks}base-security,"
                [[ "$risk_level" == "low" ]] && risk_level="medium"
                reasons="${reasons}Database workload; "
            fi

            # Check for privileged containers (CIS Kubernetes)
            if [[ "$privileged" == "true" ]] || [[ "$host_pid" == "true" ]] || [[ "$host_net" == "true" ]]; then
                detected_frameworks="${detected_frameworks}cis-kubernetes,"
                risk_level="critical"
                reasons="${reasons}Privileged container; "
            fi

            # Check for runtime socket mounts (from pre-extracted hostPath volumes)
            if echo "$host_paths" | grep -qE "docker.sock|containerd.sock"; then
                detected_frameworks="${detected_frameworks}cis-kubernetes,"
                risk_level="critical"
                reasons="${reasons}Runtime socket access; "
            fi

            # Output finding if any frameworks detected
            if [[ -n "$detected_frameworks" ]]; then
                detected_frameworks="${detected_frameworks%,}"  # Remove trailing comma
                reasons="${reasons%; }"  # Remove trailing semicolon
                findings="${findings}${pod_name}|${ns_name}|${detected_frameworks}|${risk_level}|${reasons}\n"
            fi

        done < <(echo "$pods_json" | jq -r '.items[] |
            "\(.metadata.name)|\(.metadata.namespace)|\(.metadata.labels // {} | to_entries | map("\(.key)=\(.value)") | join(","))|\(.spec.containers[].image)|\(.spec.volumes // [] | map(.hostPath.path // "") | join(","))|\(.spec.containers[0].securityContext.privileged // false)|\(.spec.hostPID // false)|\(.spec.hostNetwork // false)"' 2>/dev/null)
    done

    echo -e "$findings"
}

# Detect privileged workloads specifically
detect_privileged_workloads() {
    local namespaces="$1"
    local findings=""

    for ns in $namespaces; do
        # Find pods with privileged security context
        local privileged_pods
        privileged_pods=$(kubectl get pods -n "$ns" -o json 2>/dev/null | \
            jq -r '.items[] | select(
                .spec.containers[].securityContext.privileged == true or
                .spec.hostPID == true or
                .spec.hostNetwork == true or
                .spec.hostIPC == true
            ) | "\(.metadata.name)|\(.metadata.namespace)"' 2>/dev/null)

        if [[ -n "$privileged_pods" ]]; then
            findings="${findings}${privileged_pods}\n"
        fi
    done

    echo -e "$findings"
}

# Detect workloads with sensitive volume mounts
detect_sensitive_mounts() {
    local namespaces="$1"
    local findings=""

    local sensitive_paths="/etc/shadow|/etc/passwd|/root|/var/run/secrets|\.ssh|\.kube|credentials|secrets"

    for ns in $namespaces; do
        local pods_with_mounts
        pods_with_mounts=$(kubectl get pods -n "$ns" -o json 2>/dev/null | \
            jq -r --arg pattern "$sensitive_paths" '.items[] |
                select(.spec.volumes[]?.hostPath.path | test($pattern; "i") // false) |
                "\(.metadata.name)|\(.metadata.namespace)|\(.spec.volumes[].hostPath.path)"' 2>/dev/null)

        if [[ -n "$pods_with_mounts" ]]; then
            findings="${findings}${pods_with_mounts}\n"
        fi
    done

    echo -e "$findings"
}
