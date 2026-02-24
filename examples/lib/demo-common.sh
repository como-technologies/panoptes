#!/usr/bin/env bash
# Shared library for Panoptes example demo scripts.
# Source this from each demo.sh:
#   source "${SCRIPT_DIR}/../lib/demo-common.sh"

: "${NAMESPACE:=default}"
: "${PANOPTES_NS:=panoptes-system}"

# ─── Colors ──────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
step()  { echo -e "\n${GREEN}=== $* ===${NC}\n"; }

# ─── Panoptes install / pre-flight ───────────────────────────────────────
# Usage: panoptes_preflight [--helm-set KEY=VAL ...]
panoptes_preflight() {
    local -a helm_sets=()
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --helm-set) helm_sets+=("--set" "$2"); shift 2 ;;
            *)          shift ;;
        esac
    done

    step "Pre-flight: Checking Panoptes installation"

    if helm status panoptes -n "${PANOPTES_NS}" &>/dev/null; then
        ok "Panoptes is already installed."
    elif kubectl get namespace "${PANOPTES_NS}" &>/dev/null; then
        ok "Panoptes namespace exists."
    else
        info "Installing Panoptes..."
        helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
            -n "${PANOPTES_NS}" --create-namespace \
            "${helm_sets[@]}" \
            --wait --timeout 120s
        ok "Panoptes installed."
    fi
}

# ─── Wait for daemon pods ────────────────────────────────────────────────
# Usage: panoptes_wait_daemons argusd [janusd]
panoptes_wait_daemons() {
    info "Waiting for Panoptes daemons to be ready..."
    for daemon in "$@"; do
        kubectl wait --for=condition=ready pod \
            -l "app.kubernetes.io/name=${daemon}" \
            -n "${PANOPTES_NS}" --timeout=120s
    done
    ok "Panoptes daemons are running."
}

# ─── Deploy a workload and capture pod name ──────────────────────────────
# Usage: demo_deploy YAML_FILE DEPLOY_NAME APP_LABEL [--timeout T]
# Sets: POD (exported to caller)
demo_deploy() {
    local yaml="$1" deploy="$2" app_label="$3"
    local timeout="120s"
    if [[ "${4:-}" == "--timeout" ]]; then timeout="$5"; fi

    kubectl apply -f "${yaml}" -n "${NAMESPACE}"
    info "Waiting for ${deploy} to be ready..."
    kubectl wait --for=condition=available "${deploy}" -n "${NAMESPACE}" --timeout="${timeout}"
    ok "${deploy##*/} is running."

    POD=$(kubectl get pods -l "app=${app_label}" -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
    info "Pod: ${POD}"
}

# ─── Wait for kernel watches to initialize ───────────────────────────────
demo_init_watches() {
    local seconds="${1:-5}"
    info "Waiting ${seconds} seconds for kernel watches to initialize..."
    sleep "${seconds}"
}

# ─── Wait for events to propagate ────────────────────────────────────────
demo_propagate() {
    local seconds="${1:-3}"
    info "Waiting ${seconds} seconds for events to propagate..."
    sleep "${seconds}"
}

# ─── Show daemon logs ────────────────────────────────────────────────────
# Usage: panoptes_show_logs [--tail N] DAEMON...
panoptes_show_logs() {
    local tail=30
    if [[ "${1:-}" == "--tail" ]]; then tail="$2"; shift 2; fi

    for daemon in "$@"; do
        echo ""
        info "${daemon} logs (last ${tail} lines):"
        kubectl logs -n "${PANOPTES_NS}" -l "app.kubernetes.io/name=${daemon}" \
            --tail="${tail}" 2>/dev/null \
            || warn "Could not retrieve ${daemon} logs"
    done
}
