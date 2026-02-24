#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/demo-common.sh
source "${SCRIPT_DIR}/../lib/demo-common.sh"

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
panoptes_preflight
panoptes_wait_daemons argusd

# ─── Step 1: Deploy vulnerable workload ──────────────────────────────
step "Step 1/4: Deploying vulnerable application"

demo_deploy "${SCRIPT_DIR}/vulnerable-pod.yaml" "deploy/vulnerable-app" "vulnerable-app"

# ─── Step 2: Apply breakout detection ArgusWatcher ───────────────────
step "Step 2/4: Applying breakout detection ArgusWatcher"

kubectl apply -f "${SCRIPT_DIR}/arguswatcher.yaml" -n "${NAMESPACE}"
ok "ArgusWatcher 'breakout-detection' created."

demo_init_watches

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

# ── Attack 4: SSH Key Injection (proxy watch lifecycle demo) ─────────
# This attack demonstrates proxy watches. /root/.ssh doesn't exist at
# container startup, so the daemon watches /root (nearest ancestor).
# We split this into phases to show the proxy → promotion → detection
# lifecycle trace-by-trace.

echo ""
info "Attack 4: Injecting SSH authorized key (MITRE T1098.004 - SSH Keys)"
echo -e "         ${YELLOW}This attack demonstrates the proxy watch lifecycle.${NC}"
echo -e "         ${YELLOW}/root/.ssh doesn't exist -- the daemon watches /root instead.${NC}"
echo ""

info "Attack 4a: Verifying /root/.ssh does not exist"
kubectl exec "${POD}" -n "${NAMESPACE}" -- ls /root/.ssh 2>&1 || true
info "The daemon is using a proxy watch on /root (nearest ancestor)"
echo ""
panoptes_show_node_logs --tail 15 argusd "${POD}" "${NAMESPACE}"

sleep 2

info "Attack 4b: Creating /root/.ssh directory (triggers proxy promotion)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- mkdir -p /root/.ssh
sleep 2
info "Daemon detects .ssh creation under /root and promotes the proxy:"
echo ""
panoptes_show_node_logs --tail 15 argusd "${POD}" "${NAMESPACE}"

sleep 1

info "Attack 4c: Injecting SSH key (caught by the promoted direct watch)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ... attacker@evil.com' > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys"
ok "Injected /root/.ssh/authorized_keys"

sleep 1

# ── Attack 5: LD_PRELOAD (also uses proxy watch) ────────────────────
# /etc/ld.so.preload doesn't exist in most containers. The daemon
# watches /etc via a proxy and promotes + fires the event in one step.

echo ""
info "Attack 5: Modifying ld.so.preload (MITRE T1574.006 - LD_PRELOAD)"
echo -e "         ${YELLOW}/etc/ld.so.preload doesn't exist -- proxy watch on /etc promotes on create.${NC}"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo '/tmp/libevil.so' > /etc/ld.so.preload"
ok "Created /etc/ld.so.preload (proxy promoted + event in one step)"

sleep 1

info "Attack 6: Adding backdoor user to /etc/passwd (MITRE T1136.001 - Create Account)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- bash -c "echo 'backdoor:x:0:0:backdoor:/root:/bin/bash' >> /etc/passwd"
ok "Modified /etc/passwd"

# ─── Step 4: Show detections ────────────────────────────────────────
step "Step 4/4: Showing detected breakout indicators"

demo_propagate

info "ArgusWatcher status:"
kubectl get aw breakout-detection -n "${NAMESPACE}" -o wide 2>/dev/null || true

panoptes_show_logs --tail 40 argusd argus-operator

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}Breakout indicators detected:${NC}

  ${RED}T1074${NC}     Data Staged         /tmp/linpeas.sh, /dev/shm/payload.bin
  ${RED}T1053.003${NC} Cron Persistence    /etc/crontab modified
  ${RED}T1098.004${NC} SSH Key Injection   /root/.ssh/authorized_keys created ${YELLOW}(proxy watch)${NC}
  ${RED}T1574.006${NC} LD_PRELOAD Hijack   /etc/ld.so.preload created ${YELLOW}(proxy watch)${NC}
  ${RED}T1136.001${NC} Backdoor Account    /etc/passwd modified

${GREEN}Proxy watch lifecycle (attack 4):${NC}
  1. /root/.ssh doesn't exist     -> daemon watches /root (proxy)
  2. attacker creates /root/.ssh  -> proxy promoted to direct watch
  3. attacker writes auth keys    -> event caught by direct watch

${GREEN}In production, each of these events should trigger:${NC}
  1. Immediate alert to the security team
  2. Pod isolation (network policy or deletion)
  3. Forensic capture of pod state
  4. Investigation of the initial access vector

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
