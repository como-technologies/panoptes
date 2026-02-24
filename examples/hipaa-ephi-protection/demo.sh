#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/demo-common.sh
source "${SCRIPT_DIR}/../lib/demo-common.sh"

# ─── Cleanup ─────────────────────────────────────────────────────────
cleanup() {
    step "Cleaning up HIPAA ePHI demo resources"

    info "Deleting ePHI database deployment..."
    kubectl delete -f "${SCRIPT_DIR}/ephi-workload.yaml" --ignore-not-found -n "${NAMESPACE}"

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

# ─── Step 1: Install Panoptes with HIPAA compliance ─────────────────
step "Step 1/5: Installing Panoptes with HIPAA compliance enabled"

panoptes_preflight --helm-set compliance.hipaa.enabled=true
panoptes_wait_daemons argusd janusd

# ─── Step 2: Deploy ePHI workload ───────────────────────────────────
step "Step 2/5: Deploying ePHI database workload"

demo_deploy "${SCRIPT_DIR}/ephi-workload.yaml" "deploy/ephi-database" "ephi-database" --timeout 180s

# Give postgres time to initialize
info "Waiting 10 seconds for PostgreSQL to complete initialization..."
sleep 10

# ─── Step 3: Verify HIPAA monitoring ────────────────────────────────
step "Step 3/5: Verifying HIPAA monitoring resources"

info "ArgusWatchers with HIPAA label:"
kubectl get aw -l compliance=hipaa --all-namespaces 2>/dev/null || kubectl get aw --all-namespaces 2>/dev/null || warn "No ArgusWatchers found"

info "JanusGuards with HIPAA label:"
kubectl get jg -l compliance=hipaa --all-namespaces 2>/dev/null || kubectl get jg --all-namespaces 2>/dev/null || warn "No JanusGuards found"

demo_init_watches

# ─── Step 4: Simulate ePHI violations ───────────────────────────────
step "Step 4/5: Simulating HIPAA violations"

info "Violation 1: Unauthorized credential access (HIPAA 164.312(d) - Authentication)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- cat /etc/shadow 2>/dev/null || true
ok "Triggered: /etc/shadow access"

sleep 1

info "Violation 2: Database authentication config change (HIPAA 164.312(c)(1) - Data Integrity)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "
if [ -f /var/lib/postgresql/data/pg_hba.conf ]; then
    echo 'host all all 0.0.0.0/0 trust' >> /var/lib/postgresql/data/pg_hba.conf
fi
" || true
ok "Triggered: pg_hba.conf modification (weakened authentication)"

sleep 1

info "Violation 3: Audit log deletion (HIPAA 164.312(c)(1) - Data Integrity)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "rm -f /var/log/postgresql/*.log /var/log/dpkg.log" || true
ok "Triggered: log file deletion"

sleep 1

info "Violation 4: Authentication config tampering (HIPAA 164.312(d) - Authentication)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "
if [ -d /etc/pam.d ]; then
    echo 'auth sufficient pam_permit.so' > /etc/pam.d/common-auth
fi
" || true
ok "Triggered: PAM configuration change (authentication bypass)"

sleep 1

info "Violation 5: User account modification (HIPAA 164.312(d) - Authentication)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'attacker:x:0:0::/root:/bin/bash' >> /etc/passwd" || true
ok "Triggered: /etc/passwd modification (backdoor user)"

sleep 1

info "Violation 6: SSL certificate tampering (HIPAA 164.312(e)(1) - Transmission Security)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "mkdir -p /etc/ssl/private && echo 'FAKE CERT' > /etc/ssl/private/server.key" || true
ok "Triggered: SSL key file creation"

# ─── Step 5: Show detections ────────────────────────────────────────
step "Step 5/5: Showing HIPAA audit trail"

demo_propagate

info "ArgusWatcher status (file integrity):"
kubectl get aw --all-namespaces -o wide 2>/dev/null || true

echo ""
info "JanusGuard status (access control):"
kubectl get jg --all-namespaces -o wide 2>/dev/null || true

panoptes_show_logs --tail 30 argusd janusd

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}HIPAA violations detected:${NC}

  ${RED}164.312(d)${NC}      /etc/shadow access          Unauthorized credential read
  ${RED}164.312(c)(1)${NC}   pg_hba.conf modification    Database auth weakened to 'trust'
  ${RED}164.312(c)(1)${NC}   Log file deletion           Audit trail tampering
  ${RED}164.312(d)${NC}      PAM config modification     Authentication bypass injected
  ${RED}164.312(d)${NC}      /etc/passwd modification     Backdoor user created
  ${RED}164.312(e)(1)${NC}   SSL key creation            Certificate tampering

${GREEN}For HIPAA auditors:${NC}
  - Every event is tagged with the HIPAA requirement ID
  - Events include timestamps, pod names, and file paths
  - Export audit trail: kubectl logs -n ${PANOPTES_NS} -l app.kubernetes.io/name=argusd > audit.log
  - HIPAA requires 6 years of audit log retention

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
