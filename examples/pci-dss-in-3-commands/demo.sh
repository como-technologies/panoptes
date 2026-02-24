#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/demo-common.sh
source "${SCRIPT_DIR}/../lib/demo-common.sh"

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

panoptes_preflight --helm-set compliance.pciDss.enabled=true
panoptes_wait_daemons argusd janusd

# ─── Step 2: Deploy payment service ─────────────────────────────────
step "Step 2/5: Deploying payment-service workload"

demo_deploy "${SCRIPT_DIR}/workload.yaml" "deploy/payment-service" "payment-service"

# ─── Step 3: Verify Panoptes resources ──────────────────────────────
step "Step 3/5: Verifying PCI-DSS monitoring resources"

info "ArgusWatchers:"
kubectl get aw -n "${NAMESPACE}" 2>/dev/null || kubectl get aw --all-namespaces 2>/dev/null || warn "No ArgusWatchers found yet (they may be in panoptes-system namespace)"

info "JanusGuards:"
kubectl get jg -n "${NAMESPACE}" 2>/dev/null || kubectl get jg --all-namespaces 2>/dev/null || warn "No JanusGuards found yet (they may be in panoptes-system namespace)"

# Give the daemons a moment to set up watches
demo_init_watches

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

demo_propagate

info "ArgusWatcher status:"
kubectl get aw --all-namespaces -o wide 2>/dev/null || true

echo ""
info "JanusGuard status:"
kubectl get jg --all-namespaces -o wide 2>/dev/null || true

panoptes_show_logs --tail 30 argus-operator argusd janusd

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
