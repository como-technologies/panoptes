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
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
step()  { echo -e "\n${GREEN}=== $* ===${NC}\n"; }

# ─── Cleanup ─────────────────────────────────────────────────────────
cleanup() {
    step "Cleaning up PCI-DSS demo resources"

    info "Deleting payment-service deployment..."
    kubectl delete -f "${SCRIPT_DIR}/workload.yaml" --ignore-not-found -n "${NAMESPACE}"

    info "Uninstalling Panoptes Helm release..."
    helm uninstall panoptes -n "${PANOPTES_NS}" 2>/dev/null || true

    info "Deleting panoptes-system namespace..."
    kubectl delete namespace "${PANOPTES_NS}" --ignore-not-found

    ok "Cleanup complete."
}

if [[ "${1:-}" == "--cleanup" ]]; then
    cleanup
    exit 0
fi

# ─── Step 1: Install Panoptes ───────────────────────────────────────
step "Step 1/5: Installing Panoptes with PCI-DSS compliance enabled"

if helm status panoptes -n "${PANOPTES_NS}" &>/dev/null; then
    warn "Panoptes is already installed. Skipping Helm install."
else
    info "Installing Panoptes via Helm..."
    helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
        -n "${PANOPTES_NS}" --create-namespace \
        --set compliance.pciDss.enabled=true \
        --wait --timeout 120s
    ok "Panoptes installed."
fi

info "Waiting for Panoptes pods to be ready..."
kubectl wait --for=condition=ready pod -l app=argusd -n "${PANOPTES_NS}" --timeout=120s
kubectl wait --for=condition=ready pod -l app=janusd -n "${PANOPTES_NS}" --timeout=120s
ok "Panoptes daemons are running."

# ─── Step 2: Deploy payment service ─────────────────────────────────
step "Step 2/5: Deploying payment-service workload"

kubectl apply -f "${SCRIPT_DIR}/workload.yaml" -n "${NAMESPACE}"

info "Waiting for payment-service to be ready..."
kubectl wait --for=condition=available deploy/payment-service -n "${NAMESPACE}" --timeout=120s
ok "payment-service is running."

POD=$(kubectl get pods -l app=payment-service -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
info "Payment service pod: ${POD}"

# ─── Step 3: Verify Panoptes resources ──────────────────────────────
step "Step 3/5: Verifying PCI-DSS monitoring resources"

info "ArgusWatchers:"
kubectl get aw -n "${NAMESPACE}" 2>/dev/null || kubectl get aw --all-namespaces 2>/dev/null || warn "No ArgusWatchers found yet (they may be in panoptes-system namespace)"

info "JanusGuards:"
kubectl get jg -n "${NAMESPACE}" 2>/dev/null || kubectl get jg --all-namespaces 2>/dev/null || warn "No JanusGuards found yet (they may be in panoptes-system namespace)"

# Give the daemons a moment to set up watches
info "Waiting 5 seconds for kernel watches to initialize..."
sleep 5

# ─── Step 4: Simulate PCI-DSS violations ────────────────────────────
step "Step 4/5: Simulating PCI-DSS violations"

info "Violation 1: Modifying /etc/passwd (PCI-DSS 11.5 - Critical system file change)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo 'backdoor:x:0:0::/root:/bin/sh' >> /etc/passwd" || true
ok "Triggered: /etc/passwd modification"

sleep 1

info "Violation 2: Reading /etc/shadow (PCI-DSS 7.1 - Unauthorized credential access)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- cat /etc/shadow 2>/dev/null || true
ok "Triggered: /etc/shadow access attempt"

sleep 1

info "Violation 3: Deleting a log file (PCI-DSS 10.5.5 - Log tampering)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- rm -f /var/log/dpkg.log || true
ok "Triggered: log file deletion"

sleep 1

info "Violation 4: Modifying SSH configuration (PCI-DSS 11.5 - SSH config change)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "mkdir -p /root/.ssh && echo 'ssh-rsa AAAA... attacker@evil' > /root/.ssh/authorized_keys" || true
ok "Triggered: SSH key injection"

sleep 1

info "Violation 5: Creating file in staging area (PCI-DSS 11.5 - Staging detection)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo '#!/bin/sh' > /tmp/exploit.sh" || true
ok "Triggered: staging area file creation"

# ─── Step 5: Show detections ────────────────────────────────────────
step "Step 5/5: Showing detected events"

info "Waiting 3 seconds for events to propagate..."
sleep 3

info "ArgusWatcher status:"
kubectl get aw --all-namespaces -o wide 2>/dev/null || true

echo ""
info "Argus operator logs (last 30 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argus-operator --tail=30 2>/dev/null || warn "Could not retrieve argus-operator logs"

echo ""
info "argusd daemon logs (last 30 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argusd --tail=30 2>/dev/null || warn "Could not retrieve argusd logs"

echo ""
info "JanusGuard status:"
kubectl get jg --all-namespaces -o wide 2>/dev/null || true

echo ""
info "janusd daemon logs (last 15 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=janusd --tail=15 2>/dev/null || warn "Could not retrieve janusd logs"

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}What happened:${NC}
  1. Panoptes was installed with PCI-DSS compliance monitoring enabled
  2. A payment-service pod was deployed with the ${CYAN}pci-dss/scope: in-scope${NC} label
  3. The PCI-DSS ArgusWatcher and JanusGuard automatically targeted the pod
  4. We simulated 5 violations that map to PCI-DSS requirements
  5. Events were detected in real-time via kernel inotify/fanotify

${GREEN}Violations simulated:${NC}
  - /etc/passwd modification      -> PCI-DSS 11.5 (critical system file change)
  - /etc/shadow access            -> PCI-DSS 7.1  (unauthorized credential access)
  - /var/log/dpkg.log deletion    -> PCI-DSS 10.5.5 (log tampering)
  - /root/.ssh/authorized_keys    -> PCI-DSS 11.5 (SSH key injection)
  - /tmp/exploit.sh creation      -> PCI-DSS 11.5 (staging area activity)

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup

${YELLOW}To explore further:${NC}
  kubectl port-forward -n ${PANOPTES_NS} svc/panoptes-eye 3000:3000
  # Then open http://localhost:3000
"
