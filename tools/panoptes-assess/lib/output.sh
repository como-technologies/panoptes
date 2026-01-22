#!/bin/bash
#
# Output Formatting Functions
# Formats assessment results in various formats
#

# Output Markdown report
output_report() {
    local timestamp="$1"
    local cluster_name="$2"
    local namespaces="$3"
    local workload_findings="$4"
    local label_findings="$5"
    local existing_resources="$6"
    local gap_analysis="$7"

    local ns_count
    ns_count=$(echo "$namespaces" | wc -w)

    cat << EOF
# Panoptes Compliance Assessment Report

**Generated:** ${timestamp}
**Cluster:** ${cluster_name}
**Namespaces scanned:** ${ns_count}

---

## Executive Summary

EOF

    # Output gap analysis table with proper column alignment
    # Use text symbols for consistent alignment across terminals
    printf "| %-14s | %8s | %7s | %12s | %10s | %9s | %3s |\n" \
        "Framework" "Detected" "Labeled" "ArgusWatcher" "JanusGuard" "Enforcing" "Gap"
    printf "|%s|%s|%s|%s|%s|%s|%s|\n" \
        "----------------" "----------" "---------" "--------------" "------------" "-----------" "-----"

    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        local aw_status="No"
        local jg_status="No"
        local enf_status="No"
        [[ "$has_aw" == "true" ]] && aw_status="Yes"
        [[ "$has_jg" == "true" ]] && jg_status="Yes"
        [[ "$enforcing" == "true" ]] && enf_status="Yes"

        printf "| %-14s | %8s | %7s | %12s | %10s | %9s | %3s |\n" \
            "$framework" "$detected" "$labeled" "$aw_status" "$jg_status" "$enf_status" "$gap"
    done < <(echo -e "$gap_analysis")

    echo ""
    echo "---"
    echo ""
    echo "## Recommendations"
    echo ""

    # High priority
    local high_items
    high_items=$(echo -e "$gap_analysis" | grep "|high|" || true)
    if [[ -n "$high_items" ]]; then
        echo "### 🔴 HIGH Priority"
        echo ""
        while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
            [[ -z "$framework" ]] && continue
            echo "**${framework}**: ${recommendation}"
            echo ""

            # Show detected workloads
            echo "Detected workloads:"
            echo '```'
            echo -e "$workload_findings" | grep "${framework}" | head -5 | while IFS='|' read -r pod ns frameworks risk reasons; do
                echo "  - ${pod} (namespace: ${ns}) - ${reasons}"
            done
            echo '```'
            echo ""

            # Show commands
            echo "Commands:"
            echo '```bash'
            echo "# Label workloads"
            echo -e "$workload_findings" | grep "${framework}" | head -3 | while IFS='|' read -r pod ns frameworks risk reasons; do
                local label_key label_value
                case "$framework" in
                    pci-dss) label_key="pci-dss/scope"; label_value="in-scope" ;;
                    hipaa) label_key="hipaa/scope"; label_value="ephi" ;;
                    gdpr) label_key="gdpr/scope"; label_value="personal-data" ;;
                    soc2) label_key="soc2/scope"; label_value="in-scope" ;;
                    nist-800-53) label_key="nist-800-53/scope"; label_value="moderate" ;;
                    cis-kubernetes) label_key="cis/scope"; label_value="kubernetes-audit" ;;
                    base-security) label_key="panoptes.como-technologies.io/monitored"; label_value="true" ;;
                esac
                echo "kubectl label pod ${pod} -n ${ns} ${label_key}=${label_value}"
            done
            echo ""
            echo "# Deploy template"
            echo "kubectl apply -f deploy/compliance/${framework}/template.yaml"
            echo '```'
            echo ""
        done < <(echo -e "$high_items")
    fi

    # Medium priority
    local medium_items
    medium_items=$(echo -e "$gap_analysis" | grep "|medium|" || true)
    if [[ -n "$medium_items" ]]; then
        echo "### 🟡 MEDIUM Priority"
        echo ""
        while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
            [[ -z "$framework" ]] && continue
            echo "**${framework}**: ${recommendation} (${gap} workloads need labeling)"
            echo ""
        done < <(echo -e "$medium_items")
    fi

    # Low priority
    local low_items
    low_items=$(echo -e "$gap_analysis" | grep "|low|" || true)
    if [[ -n "$low_items" ]]; then
        echo "### 🟢 LOW Priority"
        echo ""
        while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
            [[ -z "$framework" ]] && continue
            echo "**${framework}**: ${recommendation}"
            echo ""
            echo '```bash'
            echo "kubectl apply -k deploy/compliance/${framework}-strict/"
            echo '```'
            echo ""
        done < <(echo -e "$low_items")
    fi

    # No recommendations
    if [[ -z "$high_items" ]] && [[ -z "$medium_items" ]] && [[ -z "$low_items" ]]; then
        echo "✅ No compliance gaps detected. All detected workloads are properly labeled and monitored."
        echo ""
    fi

    echo "---"
    echo ""
    echo "## Detailed Findings"
    echo ""
    echo "### Detected Workloads"
    echo ""

    local finding_count
    finding_count=$(echo -e "$workload_findings" | grep -c "." 2>/dev/null) || finding_count=0

    if [[ $finding_count -gt 0 ]]; then
        # Column widths: Pod=40, Namespace=20, Frameworks=20, Risk=8, Reason=40
        printf "| %-40s | %-20s | %-20s | %-8s | %-40s |\n" \
            "Pod" "Namespace" "Frameworks" "Risk" "Reason"
        printf "|%s|%s|%s|%s|%s|\n" \
            "------------------------------------------" "----------------------" "----------------------" "----------" "------------------------------------------"
        echo -e "$workload_findings" | head -20 | while IFS='|' read -r pod ns frameworks risk reasons; do
            [[ -z "$pod" ]] && continue
            # Truncate long values to fit columns
            pod="${pod:0:40}"
            ns="${ns:0:20}"
            frameworks="${frameworks:0:20}"
            reasons="${reasons:0:40}"
            printf "| %-40s | %-20s | %-20s | %-8s | %-40s |\n" \
                "$pod" "$ns" "$frameworks" "$risk" "$reasons"
        done
        if [[ $finding_count -gt 20 ]]; then
            echo ""
            echo "_... and $((finding_count - 20)) more workloads_"
        fi
    else
        echo "_No workloads requiring compliance monitoring detected._"
    fi

    echo ""
    echo "---"
    echo ""
    echo "_Report generated by Panoptes Compliance Assessment Tool_"
    echo "_https://github.com/como-technologies/panoptes_"
}

# Output JSON format
output_json() {
    local timestamp="$1"
    local cluster_name="$2"
    local namespaces="$3"
    local workload_findings="$4"
    local label_findings="$5"
    local existing_resources="$6"
    local gap_analysis="$7"

    # Build JSON output
    cat << EOF
{
  "assessment": {
    "timestamp": "${timestamp}",
    "cluster": "${cluster_name}",
    "namespacesScanned": $(echo "$namespaces" | wc -w),
    "findings": {
      "workloads": [
EOF

    # Output workload findings as JSON array
    local first=true
    echo -e "$workload_findings" | while IFS='|' read -r pod ns frameworks risk reasons; do
        [[ -z "$pod" ]] && continue
        if [[ "$first" == true ]]; then
            first=false
        else
            echo ","
        fi
        cat << ITEM
        {
          "pod": "${pod}",
          "namespace": "${ns}",
          "frameworks": [$(echo "$frameworks" | sed 's/,/","/g' | sed 's/^/"/' | sed 's/$/"/')],
          "riskLevel": "${risk}",
          "reasons": "${reasons}"
        }
ITEM
    done

    cat << EOF
      ],
      "gapAnalysis": [
EOF

    # Output gap analysis
    first=true
    echo -e "$gap_analysis" | while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        if [[ "$first" == true ]]; then
            first=false
        else
            echo ","
        fi
        cat << ITEM
        {
          "framework": "${framework}",
          "detected": ${detected},
          "labeled": ${labeled},
          "hasArgusWatcher": ${has_aw},
          "hasJanusGuard": ${has_jg},
          "enforcing": ${enforcing},
          "gap": ${gap},
          "priority": "${priority}",
          "recommendation": "${recommendation}"
        }
ITEM
    done

    cat << EOF
      ]
    },
    "recommendations": [
EOF

    # Output recommendations
    first=true
    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        if [[ "$first" == true ]]; then
            first=false
        else
            echo ","
        fi
        cat << ITEM
      {
        "priority": "${priority}",
        "framework": "${framework}",
        "action": "${recommendation}"
      }
ITEM
    done < <(echo -e "$gap_analysis" | grep -v "|none|" || true)

    cat << EOF
    ]
  }
}
EOF
}

# Output YAML format (ready-to-apply manifests)
output_yaml() {
    local workload_findings="$1"
    local label_findings="$2"
    local gap_analysis="$3"

    echo "# Panoptes Compliance Assessment - Generated Labels"
    echo "# Apply with: kubectl apply -f <this-file>"
    echo "#"
    echo "# WARNING: This will modify pod labels. Review before applying."
    echo "---"

    # Generate label patches for detected workloads
    echo -e "$workload_findings" | while IFS='|' read -r pod ns frameworks risk reasons; do
        [[ -z "$pod" ]] && continue

        # Get owner reference to label the controller instead of pod
        local owner_kind owner_name
        owner_kind=$(kubectl get pod "$pod" -n "$ns" -o jsonpath='{.metadata.ownerReferences[0].kind}' 2>/dev/null || echo "")
        owner_name=$(kubectl get pod "$pod" -n "$ns" -o jsonpath='{.metadata.ownerReferences[0].name}' 2>/dev/null || echo "")

        # Determine labels to add
        local labels=""
        for framework in $(echo "$frameworks" | tr ',' ' '); do
            case "$framework" in
                pci-dss) labels="${labels}    pci-dss/scope: in-scope\n" ;;
                hipaa) labels="${labels}    hipaa/scope: ephi\n" ;;
                gdpr) labels="${labels}    gdpr/scope: personal-data\n" ;;
                soc2) labels="${labels}    soc2/scope: in-scope\n" ;;
                nist-800-53) labels="${labels}    nist-800-53/scope: moderate\n" ;;
                cis-kubernetes) labels="${labels}    cis/scope: kubernetes-audit\n" ;;
                base-security) labels="${labels}    panoptes.como-technologies.io/monitored: \"true\"\n" ;;
            esac
        done

        if [[ -n "$owner_kind" ]] && [[ "$owner_kind" == "ReplicaSet" ]]; then
            # Get deployment name from ReplicaSet
            local deploy_name
            deploy_name=$(kubectl get replicaset "$owner_name" -n "$ns" -o jsonpath='{.metadata.ownerReferences[0].name}' 2>/dev/null || echo "")
            if [[ -n "$deploy_name" ]]; then
                cat << EOF
# ${deploy_name} in ${ns} - ${reasons}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ${deploy_name}
  namespace: ${ns}
spec:
  template:
    metadata:
      labels:
$(echo -e "$labels")
---
EOF
            fi
        else
            # Label pod directly (for standalone pods)
            cat << EOF
# ${pod} in ${ns} - ${reasons}
apiVersion: v1
kind: Pod
metadata:
  name: ${pod}
  namespace: ${ns}
  labels:
$(echo -e "$labels")
---
EOF
        fi
    done

    echo ""
    echo "# Recommended compliance templates to deploy:"

    # List templates to deploy
    while IFS='|' read -r framework detected labeled has_aw has_jg enforcing gap priority recommendation; do
        [[ -z "$framework" ]] && continue
        echo "# kubectl apply -f deploy/compliance/${framework}/template.yaml"
    done < <(echo -e "$gap_analysis" | grep "|high|" || true)
}
