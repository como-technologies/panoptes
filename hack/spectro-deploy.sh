#!/bin/bash
# Panoptes Spectro Cloud Palette Deployment Script
# Deploy Panoptes security suite to managed Kubernetes clusters via Spectro Cloud
#
# Usage: ./hack/spectro-deploy.sh [login|pack-push|profile-create|deploy|status|clean]
#
# Prerequisites:
# - Spectro Cloud account with Palette access
# - `palette` CLI installed (https://docs.spectrocloud.com/palette-cli)
# - Or use PALETTE_API_KEY environment variable for API access
#
# Copyright 2026 Como Technologies, LTD
# Licensed under Apache License 2.0

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PACKS_DIR="${ROOT_DIR}/packs"

# Spectro Cloud defaults
PALETTE_ENDPOINT="${PALETTE_ENDPOINT:-api.spectrocloud.com}"
PALETTE_PROJECT="${PALETTE_PROJECT:-Default}"
PACK_VERSION="${PACK_VERSION:-2.0.0}"
REGISTRY_NAME="${REGISTRY_NAME:-spectro-packs}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_spectro() { echo -e "${CYAN}[SPECTRO]${NC} $1"; }

# Check prerequisites
check_prereqs() {
    log_info "Checking prerequisites..."

    # Check for palette CLI or API key
    if command -v palette >/dev/null 2>&1; then
        log_info "Found palette CLI."
        USE_CLI=true
    elif [[ -n "${PALETTE_API_KEY:-}" ]]; then
        log_info "Using PALETTE_API_KEY for API access."
        USE_CLI=false
        # Check for curl and jq
        command -v curl >/dev/null 2>&1 || { log_error "curl is required for API access."; exit 1; }
        command -v jq >/dev/null 2>&1 || { log_error "jq is required for API access."; exit 1; }
    else
        log_error "Neither 'palette' CLI nor PALETTE_API_KEY found."
        log_info "Install palette CLI: https://docs.spectrocloud.com/palette-cli"
        log_info "Or set PALETTE_API_KEY environment variable."
        exit 1
    fi

    # Check for helm (needed for pack validation)
    command -v helm >/dev/null 2>&1 || { log_warn "helm not found. Pack validation will be skipped."; }

    # Check packs directory exists
    if [[ ! -d "${PACKS_DIR}" ]]; then
        log_error "Packs directory not found: ${PACKS_DIR}"
        exit 1
    fi

    log_info "Prerequisites check passed."
}

# Login to Palette
do_login() {
    log_spectro "Logging in to Spectro Cloud Palette..."

    if [[ "${USE_CLI:-false}" == "true" ]]; then
        palette login --console "${PALETTE_ENDPOINT}"
    else
        # Test API connectivity
        local response
        response=$(curl -s -o /dev/null -w "%{http_code}" \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/users/me")

        if [[ "${response}" == "200" ]]; then
            log_info "API authentication successful."
        else
            log_error "API authentication failed. HTTP status: ${response}"
            exit 1
        fi
    fi

    log_info "Login successful."
}

# Validate pack structure
validate_pack() {
    local pack_dir="$1"
    local pack_name=$(basename "${pack_dir}")

    log_info "Validating pack: ${pack_name}..."

    # Check required files
    [[ -f "${pack_dir}/pack.json" ]] || { log_error "Missing pack.json in ${pack_name}"; return 1; }
    [[ -f "${pack_dir}/values.yaml" ]] || { log_error "Missing values.yaml in ${pack_name}"; return 1; }

    # Validate Helm chart if exists
    if [[ -d "${pack_dir}/charts" ]] && command -v helm >/dev/null 2>&1; then
        for chart in "${pack_dir}"/charts/*/; do
            if [[ -f "${chart}/Chart.yaml" ]]; then
                helm lint "${chart}" || { log_error "Helm lint failed for ${chart}"; return 1; }
            fi
        done
    fi

    log_info "Pack ${pack_name} validation passed."
    return 0
}

# Push packs to Spectro Cloud registry
push_packs() {
    log_spectro "Pushing packs to Spectro Cloud registry..."

    local packs=("panoptes" "argus-fim" "janus-audit")

    for pack in "${packs[@]}"; do
        local pack_dir="${PACKS_DIR}/${pack}"

        if [[ ! -d "${pack_dir}" ]]; then
            log_warn "Pack directory not found: ${pack_dir}. Skipping."
            continue
        fi

        # Validate pack
        validate_pack "${pack_dir}" || continue

        log_info "Pushing pack: ${pack}..."

        if [[ "${USE_CLI:-false}" == "true" ]]; then
            # Use palette CLI
            palette pack push "${pack_dir}" \
                --registry-name "${REGISTRY_NAME}" \
                --version "${PACK_VERSION}" \
                --force || { log_error "Failed to push ${pack}"; continue; }
        else
            # Use API
            local pack_json
            pack_json=$(cat "${pack_dir}/pack.json")

            local response
            response=$(curl -s -X POST \
                -H "Authorization: Bearer ${PALETTE_API_KEY}" \
                -H "Content-Type: application/json" \
                -d "${pack_json}" \
                "https://${PALETTE_ENDPOINT}/v1/packs")

            if echo "${response}" | jq -e '.metadata.uid' >/dev/null 2>&1; then
                log_info "Pack ${pack} pushed successfully."
            else
                log_error "Failed to push ${pack}: ${response}"
            fi
        fi
    done

    log_info "Pack push complete."
}

# Create cluster profile with Panoptes
create_profile() {
    local profile_name="${1:-panoptes-security}"
    local preset="${2:-default}"

    log_spectro "Creating cluster profile: ${profile_name}..."

    if [[ "${USE_CLI:-false}" == "true" ]]; then
        # Use palette CLI
        palette profile create \
            --name "${profile_name}" \
            --type cluster \
            --pack "panoptes:${PACK_VERSION}" \
            --project "${PALETTE_PROJECT}" || { log_error "Failed to create profile"; return 1; }
    else
        # Use API
        local preset_file="${PACKS_DIR}/panoptes/presets/${preset}.yaml"
        local values_content=""

        if [[ -f "${preset_file}" ]]; then
            values_content=$(cat "${preset_file}" | jq -Rs .)
        else
            values_content=$(cat "${PACKS_DIR}/panoptes/values.yaml" | jq -Rs .)
        fi

        local payload
        payload=$(cat <<EOF
{
  "metadata": {
    "name": "${profile_name}"
  },
  "spec": {
    "type": "cluster",
    "packs": [
      {
        "name": "panoptes",
        "version": "${PACK_VERSION}",
        "values": ${values_content}
      }
    ]
  }
}
EOF
)

        local response
        response=$(curl -s -X POST \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            -H "Content-Type: application/json" \
            -d "${payload}" \
            "https://${PALETTE_ENDPOINT}/v1/clusterprofiles")

        if echo "${response}" | jq -e '.metadata.uid' >/dev/null 2>&1; then
            local uid
            uid=$(echo "${response}" | jq -r '.metadata.uid')
            log_info "Profile created successfully. UID: ${uid}"
        else
            log_error "Failed to create profile: ${response}"
            return 1
        fi
    fi

    log_info "Cluster profile '${profile_name}' created."
}

# Deploy to a managed cluster
deploy_to_cluster() {
    local cluster_name="${1:-}"
    local profile_name="${2:-panoptes-security}"

    if [[ -z "${cluster_name}" ]]; then
        log_error "Cluster name required. Usage: $0 deploy <cluster-name> [profile-name]"
        exit 1
    fi

    log_spectro "Deploying Panoptes to cluster: ${cluster_name}..."

    if [[ "${USE_CLI:-false}" == "true" ]]; then
        # Use palette CLI to attach profile
        palette cluster profile attach \
            --cluster-name "${cluster_name}" \
            --profile-name "${profile_name}" \
            --project "${PALETTE_PROJECT}" || { log_error "Failed to attach profile"; return 1; }
    else
        # Get cluster UID
        local cluster_response
        cluster_response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters?name=${cluster_name}")

        local cluster_uid
        cluster_uid=$(echo "${cluster_response}" | jq -r '.items[0].metadata.uid // empty')

        if [[ -z "${cluster_uid}" ]]; then
            log_error "Cluster '${cluster_name}' not found."
            return 1
        fi

        # Get profile UID
        local profile_response
        profile_response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/clusterprofiles?name=${profile_name}")

        local profile_uid
        profile_uid=$(echo "${profile_response}" | jq -r '.items[0].metadata.uid // empty')

        if [[ -z "${profile_uid}" ]]; then
            log_error "Profile '${profile_name}' not found."
            return 1
        fi

        # Attach profile to cluster
        local response
        response=$(curl -s -X PATCH \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            -H "Content-Type: application/json" \
            -d "{\"profiles\": [{\"uid\": \"${profile_uid}\"}]}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters/${cluster_uid}/profiles")

        log_info "Profile attached to cluster."
    fi

    log_info "Deployment initiated. Use '$0 status ${cluster_name}' to check status."
}

# Check deployment status
check_status() {
    local cluster_name="${1:-}"

    if [[ -z "${cluster_name}" ]]; then
        log_error "Cluster name required. Usage: $0 status <cluster-name>"
        exit 1
    fi

    log_spectro "Checking Panoptes deployment status on: ${cluster_name}..."

    if [[ "${USE_CLI:-false}" == "true" ]]; then
        palette cluster get --name "${cluster_name}" --project "${PALETTE_PROJECT}"
    else
        local response
        response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters?name=${cluster_name}")

        local cluster_uid
        cluster_uid=$(echo "${response}" | jq -r '.items[0].metadata.uid // empty')

        if [[ -z "${cluster_uid}" ]]; then
            log_error "Cluster '${cluster_name}' not found."
            return 1
        fi

        # Get detailed status
        local detail_response
        detail_response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters/${cluster_uid}")

        echo "${detail_response}" | jq '{
            name: .metadata.name,
            status: .status.state,
            health: .status.health.state,
            profiles: [.spec.profiles[]? | {name: .name, version: .version, status: .status}]
        }'
    fi
}

# Remove Panoptes from cluster
clean_cluster() {
    local cluster_name="${1:-}"
    local profile_name="${2:-panoptes-security}"

    if [[ -z "${cluster_name}" ]]; then
        log_error "Cluster name required. Usage: $0 clean <cluster-name> [profile-name]"
        exit 1
    fi

    log_spectro "Removing Panoptes from cluster: ${cluster_name}..."

    if [[ "${USE_CLI:-false}" == "true" ]]; then
        palette cluster profile detach \
            --cluster-name "${cluster_name}" \
            --profile-name "${profile_name}" \
            --project "${PALETTE_PROJECT}" || { log_error "Failed to detach profile"; return 1; }
    else
        # Get cluster UID
        local cluster_response
        cluster_response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters?name=${cluster_name}")

        local cluster_uid
        cluster_uid=$(echo "${cluster_response}" | jq -r '.items[0].metadata.uid // empty')

        if [[ -z "${cluster_uid}" ]]; then
            log_error "Cluster '${cluster_name}' not found."
            return 1
        fi

        # Get profile UID to remove
        local profile_response
        profile_response=$(curl -s \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/clusterprofiles?name=${profile_name}")

        local profile_uid
        profile_uid=$(echo "${profile_response}" | jq -r '.items[0].metadata.uid // empty')

        if [[ -z "${profile_uid}" ]]; then
            log_warn "Profile '${profile_name}' not found. May already be removed."
            return 0
        fi

        # Detach profile
        local response
        response=$(curl -s -X DELETE \
            -H "Authorization: Bearer ${PALETTE_API_KEY}" \
            "https://${PALETTE_ENDPOINT}/v1/spectroclusters/${cluster_uid}/profiles/${profile_uid}")

        log_info "Profile detached from cluster."
    fi

    log_info "Panoptes removed from cluster '${cluster_name}'."
}

# List available packs
list_packs() {
    log_spectro "Available Panoptes Packs:"
    echo ""

    for pack_dir in "${PACKS_DIR}"/*/; do
        if [[ -f "${pack_dir}/pack.json" ]]; then
            local pack_name
            pack_name=$(jq -r '.name' "${pack_dir}/pack.json")
            local display_name
            display_name=$(jq -r '.displayName' "${pack_dir}/pack.json")
            local version
            version=$(jq -r '.version' "${pack_dir}/pack.json")
            local description
            description=$(jq -r '.annotations.description // "No description"' "${pack_dir}/pack.json")

            echo -e "  ${GREEN}${pack_name}${NC} (v${version})"
            echo -e "    ${display_name}"
            echo -e "    ${description}"
            echo ""
        fi
    done
}

# Show usage
usage() {
    cat <<EOF
Panoptes Spectro Cloud Palette Deployment Script

Deploy Panoptes security suite to managed Kubernetes clusters via Spectro Cloud.

Usage: $0 [command] [options]

Commands:
    login                          Login to Spectro Cloud Palette
    pack-push                      Push all packs to Spectro Cloud registry
    profile-create [name] [preset] Create cluster profile (default: panoptes-security, default)
    deploy <cluster> [profile]     Deploy Panoptes to managed cluster
    status <cluster>               Check deployment status
    clean <cluster> [profile]      Remove Panoptes from cluster
    list                           List available packs

Presets:
    default     Standard deployment with all components
    compliance  PCI-DSS/SOC2 optimized with extended retention
    minimal     Argus only, no dashboard

Environment Variables:
    PALETTE_API_KEY     API key for Spectro Cloud (alternative to CLI login)
    PALETTE_ENDPOINT    API endpoint (default: api.spectrocloud.com)
    PALETTE_PROJECT     Project name (default: Default)
    PACK_VERSION        Pack version to use (default: 2.0.0)
    REGISTRY_NAME       Pack registry name (default: spectro-packs)

Examples:
    # Push packs and create profile
    $0 login
    $0 pack-push
    $0 profile-create production-security compliance

    # Deploy to cluster
    $0 deploy my-prod-cluster production-security
    $0 status my-prod-cluster

    # Using API key
    export PALETTE_API_KEY="your-api-key"
    $0 deploy my-cluster panoptes-security

    # Clean up
    $0 clean my-cluster panoptes-security

Documentation:
    https://docs.spectrocloud.com/
    See docs/SPECTRO_QUICK_START.md for detailed instructions.
EOF
}

# Main
main() {
    local command="${1:-}"
    shift || true

    case "${command}" in
        login)
            check_prereqs
            do_login
            ;;
        pack-push)
            check_prereqs
            push_packs
            ;;
        profile-create)
            check_prereqs
            create_profile "$@"
            ;;
        deploy)
            check_prereqs
            deploy_to_cluster "$@"
            ;;
        status)
            check_prereqs
            check_status "$@"
            ;;
        clean)
            check_prereqs
            clean_cluster "$@"
            ;;
        list)
            list_packs
            ;;
        -h|--help|help|"")
            usage
            ;;
        *)
            log_error "Unknown command: ${command}"
            usage
            exit 1
            ;;
    esac
}

main "$@"
