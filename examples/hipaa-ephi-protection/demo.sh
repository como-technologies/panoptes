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

if helm status panoptes -n "${PANOPTES_NS}" &>/dev/null; then
    warn "Panoptes is already installed. Skipping Helm install."
else
    info "Installing Panoptes with HIPAA compliance..."
    helm install panoptes oci://ghcr.io/como-technologies/charts/panoptes \
        -n "${PANOPTES_NS}" --create-namespace \
        --set compliance.hipaa.enabled=true \
        --wait --timeout 120s
    ok "Panoptes installed with HIPAA compliance."
fi

info "Waiting for Panoptes daemons to be ready..."
kubectl wait --for=condition=ready pod -l app=argusd -n "${PANOPTES_NS}" --timeout=120s
kubectl wait --for=condition=ready pod -l app=janusd -n "${PANOPTES_NS}" --timeout=120s
ok "Panoptes daemons are running."

# ─── Step 2: Deploy ePHI workload ───────────────────────────────────
step "Step 2/5: Deploying ePHI database workload"

kubectl apply -f "${SCRIPT_DIR}/ephi-workload.yaml" -n "${NAMESPACE}"

info "Waiting for ephi-database to be ready..."
kubectl wait --for=condition=available deploy/ephi-database -n "${NAMESPACE}" --timeout=180s
ok "ephi-database is running."

POD=$(kubectl get pods -l app=ephi-database -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
info "ePHI database pod: ${POD}"

# Give postgres time to initialize
info "Waiting 10 seconds for PostgreSQL to complete initialization..."
sleep 10

# ─── Step 3: Verify HIPAA monitoring ────────────────────────────────
step "Step 3/5: Verifying HIPAA monitoring resources"

info "ArgusWatchers with HIPAA label:"
kubectl get aw -l compliance=hipaa --all-namespaces 2>/dev/null || kubectl get aw --all-namespaces 2>/dev/null || warn "No ArgusWatchers found"

info "JanusGuards with HIPAA label:"
kubectl get jg -l compliance=hipaa --all-namespaces 2>/dev/null || kubectl get jg --all-namespaces 2>/dev/null || warn "No JanusGuards found"

info "Waiting 5 seconds for kernel watches to initialize..."
sleep 5

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

info "Waiting 3 seconds for events to propagate..."
sleep 3

info "ArgusWatcher status (file integrity):"
kubectl get aw --all-namespaces -o wide 2>/dev/null || true

echo ""
info "argusd daemon logs (file integrity events, last 30 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argusd --tail=30 2>/dev/null || warn "Could not retrieve argusd logs"

echo ""
info "JanusGuard status (access control):"
kubectl get jg --all-namespaces -o wide 2>/dev/null || true

echo ""
info "janusd daemon logs (access audit events, last 15 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=janusd --tail=15 2>/dev/null || warn "Could not retrieve janusd logs"

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
  - Export audit trail: kubectl logs -n ${PANOPTES_NS} -l app=argusd > audit.log
  - HIPAA requires 6 years of audit log retention

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
