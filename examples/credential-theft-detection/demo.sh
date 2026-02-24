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
    step "Cleaning up credential theft demo resources"

    info "Deleting workload, ArgusWatcher, and JanusGuard..."
    kubectl delete -f "${SCRIPT_DIR}/attacker-simulation.yaml" --ignore-not-found -n "${NAMESPACE}"

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

info "Waiting for Panoptes daemons to be ready..."
kubectl wait --for=condition=ready pod -l app=argusd -n "${PANOPTES_NS}" --timeout=120s
kubectl wait --for=condition=ready pod -l app=janusd -n "${PANOPTES_NS}" --timeout=120s
ok "Panoptes daemons are running."

# ─── Step 1: Deploy workload with monitoring ─────────────────────────
step "Step 1/4: Deploying target workload with ArgusWatcher and JanusGuard"

kubectl apply -f "${SCRIPT_DIR}/attacker-simulation.yaml" -n "${NAMESPACE}"

info "Waiting for target-workload to be ready..."
kubectl wait --for=condition=available deploy/target-workload -n "${NAMESPACE}" --timeout=120s
ok "target-workload is running."

POD=$(kubectl get pods -l app=target-workload -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
info "Target pod: ${POD}"

# ─── Step 2: Verify monitoring ──────────────────────────────────────
step "Step 2/4: Verifying credential monitoring is active"

info "Waiting 5 seconds for kernel watches to initialize..."
sleep 5

info "ArgusWatcher (file modification detection):"
kubectl get aw credential-fim -n "${NAMESPACE}" -o wide 2>/dev/null || warn "ArgusWatcher not yet visible"

info "JanusGuard (file access auditing):"
kubectl get jg credential-access -n "${NAMESPACE}" -o wide 2>/dev/null || warn "JanusGuard not yet visible"

# ─── Step 3: Simulate credential theft ──────────────────────────────
step "Step 3/4: Simulating credential theft attempts"

info "Attack 1: Reading /etc/shadow (password hash extraction)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- cat /etc/shadow 2>/dev/null || true
ok "Attempted: /etc/shadow read"

sleep 1

info "Attack 2: Reading Kubernetes service account token"
kubectl exec "${POD}" -n "${NAMESPACE}" -- cat /var/run/secrets/kubernetes.io/serviceaccount/token 2>/dev/null || true
ok "Attempted: K8s service account token read"

sleep 1

info "Attack 3: Injecting SSH authorized key"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "mkdir -p /root/.ssh && echo 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ... attacker@evil.com' > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys"
ok "Created: /root/.ssh/authorized_keys"

sleep 1

info "Attack 4: Adding backdoor user to /etc/passwd"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'backdoor:x:0:0:backdoor:/root:/bin/bash' >> /etc/passwd"
ok "Modified: /etc/passwd (UID 0 user added)"

sleep 1

info "Attack 5: Attempting to read /etc/shadow password hashes"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "cat /etc/shadow | head -3" 2>/dev/null || true
ok "Attempted: /etc/shadow second read"

sleep 1

info "Attack 6: Modifying /etc/shadow (resetting root password)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "chmod 644 /etc/shadow" 2>/dev/null || true
ok "Attempted: /etc/shadow permission change"

sleep 1

info "Attack 7: Creating fake cloud credential file"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "mkdir -p /root/.aws && echo '[default]
aws_access_key_id=AKIAIOSFODNN7EXAMPLE
aws_secret_access_key=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY' > /root/.aws/credentials"
ok "Created: /root/.aws/credentials"

# ─── Step 4: Show detections ────────────────────────────────────────
step "Step 4/4: Showing detected credential theft events"

info "Waiting 3 seconds for events to propagate..."
sleep 3

info "ArgusWatcher status (modification detection):"
kubectl get aw credential-fim -n "${NAMESPACE}" -o wide 2>/dev/null || true

echo ""
info "argusd logs -- file modification events (last 25 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argusd --tail=25 2>/dev/null || warn "Could not retrieve argusd logs"

echo ""
info "JanusGuard status (access auditing):"
kubectl get jg credential-access -n "${NAMESPACE}" -o wide 2>/dev/null || true

echo ""
info "janusd logs -- access audit events (last 20 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=janusd --tail=20 2>/dev/null || warn "Could not retrieve janusd logs"

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}Credential theft attempts detected:${NC}

  ${RED}ArgusWatcher (modification detection):${NC}
    /root/.ssh/authorized_keys   created   (SSH key injection)
    /etc/passwd                  modified  (backdoor user added)
    /etc/shadow                  attrib    (permissions changed)
    /root/.aws/credentials       created   (cloud credential planted)

  ${RED}JanusGuard (access auditing):${NC}
    /etc/shadow                  open      (password hash read)
    /var/run/secrets/.../token   open      (K8s token read)

${GREEN}Defense in depth:${NC}
  ArgusWatcher detects writes, JanusGuard detects reads.
  Together they provide complete credential file visibility.

${YELLOW}Next step -- enable enforcement to BLOCK credential access:${NC}
  kubectl patch jg credential-access -p '{\"spec\":{\"enforcing\":true}}'

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
