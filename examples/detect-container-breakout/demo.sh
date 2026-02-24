#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NAMESPACE="default"
PANOPTES_NS="panoptes-system"

# ─── Colors ──────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
step()  { echo -e "\n${GREEN}=== $* ===${NC}\n"; }

# ─── Cleanup ─────────────────────────────────────────────────────────
cleanup() {
    step "Cleaning up container breakout demo resources"

    info "Deleting ArgusWatcher..."
    kubectl delete -f "${SCRIPT_DIR}/arguswatcher.yaml" --ignore-not-found -n "${NAMESPACE}"

    info "Deleting vulnerable-app deployment..."
    kubectl delete -f "${SCRIPT_DIR}/vulnerable-pod.yaml" --ignore-not-found -n "${NAMESPACE}"

    ok "Cleanup complete."
    echo -e "${YELLOW}Note:${NC} Panoptes itself was not uninstalled. Run 'helm uninstall panoptes -n ${PANOPTES_NS}' to remove it."
}

if [[ "${1:-}" == "--cleanup" ]]; then
    cleanup
    exit 0
fi

# ─── Pre-flight check ───────────────────────────────────────────────
step "Pre-flight: Checking Panoptes installation"

if ! kubectl get namespace "${PANOPTES_NS}" &>/dev/null; then
    warn "Panoptes namespace '${PANOPTES_NS}' not found."
    info "Installing Panoptes..."
    helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
        -n "${PANOPTES_NS}" --create-namespace \
        --wait --timeout 120s
    ok "Panoptes installed."
else
    ok "Panoptes namespace exists."
fi

info "Waiting for argusd to be ready..."
kubectl wait --for=condition=ready pod -l app=argusd -n "${PANOPTES_NS}" --timeout=120s
ok "argusd is running."

# ─── Step 1: Deploy vulnerable workload ──────────────────────────────
step "Step 1/4: Deploying vulnerable application"

kubectl apply -f "${SCRIPT_DIR}/vulnerable-pod.yaml" -n "${NAMESPACE}"

info "Waiting for vulnerable-app to be ready..."
kubectl wait --for=condition=available deploy/vulnerable-app -n "${NAMESPACE}" --timeout=120s
ok "vulnerable-app is running."

POD=$(kubectl get pods -l app=vulnerable-app -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
info "Vulnerable pod: ${POD}"

# ─── Step 2: Apply breakout detection ArgusWatcher ───────────────────
step "Step 2/4: Applying breakout detection ArgusWatcher"

kubectl apply -f "${SCRIPT_DIR}/arguswatcher.yaml" -n "${NAMESPACE}"
ok "ArgusWatcher 'breakout-detection' created."

info "Waiting 5 seconds for kernel watches to initialize..."
sleep 5

kubectl get aw breakout-detection -n "${NAMESPACE}" -o wide 2>/dev/null || true

# ─── Step 3: Simulate container breakout ─────────────────────────────
step "Step 3/4: Simulating container breakout attempt"

info "Attack 1: Staging tools in /tmp (MITRE T1074 - Data Staged)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo '#!/bin/bash
# linpeas - Linux Privilege Escalation Awesome Script
echo Running enumeration...' > /tmp/linpeas.sh && chmod +x /tmp/linpeas.sh"
ok "Staged /tmp/linpeas.sh"

sleep 1

info "Attack 2: Staging payload in /dev/shm (MITRE T1074 - Data Staged)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'exploit_payload_data' > /dev/shm/payload.bin"
ok "Staged /dev/shm/payload.bin"

sleep 1

info "Attack 3: Adding cron job for persistence (MITRE T1053.003 - Cron)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo '* * * * * root /tmp/linpeas.sh' >> /etc/crontab" || true
ok "Modified /etc/crontab"

sleep 1

info "Attack 4: Injecting SSH authorized key (MITRE T1098.004 - SSH Keys)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "mkdir -p /root/.ssh && echo 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ... attacker@evil.com' > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys"
ok "Injected /root/.ssh/authorized_keys"

sleep 1

info "Attack 5: Modifying ld.so.preload (MITRE T1574.006 - LD_PRELOAD)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo '/tmp/libevil.so' > /etc/ld.so.preload"
ok "Modified /etc/ld.so.preload"

sleep 1

info "Attack 6: Adding backdoor user to /etc/passwd (MITRE T1136.001 - Create Account)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'backdoor:x:0:0:backdoor:/root:/bin/bash' >> /etc/passwd"
ok "Modified /etc/passwd"

# ─── Step 4: Show detections ────────────────────────────────────────
step "Step 4/4: Showing detected breakout indicators"

info "Waiting 3 seconds for events to propagate..."
sleep 3

info "ArgusWatcher status:"
kubectl get aw breakout-detection -n "${NAMESPACE}" -o wide 2>/dev/null || true

echo ""
info "argusd daemon logs (breakout-related events):"
kubectl logs -n "${PANOPTES_NS}" -l app=argusd --tail=40 2>/dev/null || warn "Could not retrieve argusd logs"

echo ""
info "Argus operator logs (last 20 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argus-operator --tail=20 2>/dev/null || warn "Could not retrieve argus-operator logs"

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}Breakout indicators detected:${NC}

  ${RED}T1074${NC}     Data Staged         /tmp/linpeas.sh, /dev/shm/payload.bin
  ${RED}T1053.003${NC} Cron Persistence    /etc/crontab modified
  ${RED}T1098.004${NC} SSH Key Injection   /root/.ssh/authorized_keys created
  ${RED}T1574.006${NC} LD_PRELOAD Hijack   /etc/ld.so.preload modified
  ${RED}T1136.001${NC} Backdoor Account    /etc/passwd modified

${GREEN}In production, each of these events should trigger:${NC}
  1. Immediate alert to the security team
  2. Pod isolation (network policy or deletion)
  3. Forensic capture of pod state
  4. Investigation of the initial access vector

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
