#!/bin/bash
# Panoptes Local Development Deployment Script (WSL2 Variant)
# For use with Windows Subsystem for Linux 2
#
# Usage: ./hack/local-wsl-deploy.sh [build|deploy|test|clean|all]
#
# Differences from local-deploy.sh:
# - Uses kind-config-wsl.yaml with pinned K8s version
# - WSL2-specific prerequisites checking
# - Handles WSL2 networking quirks
#
# Copyright 2026 Como Technologies, LTD
# Licensed under Apache License 2.0

set -euo pipefail

# Configuration
CLUSTER_NAME="${CLUSTER_NAME:-panoptes-dev}"
NAMESPACE="${NAMESPACE:-panoptes-system}"
IMAGE_TAG="${IMAGE_TAG:-dev}"
# Image variant selection - default to slim for WSL2 (no BTF support in standard kernel)
IMAGE_VARIANT="${IMAGE_VARIANT:-slim}"
# Runtime mode override (auto=detect at runtime, ebpf=force eBPF, traditional=force fanotify)
DAEMON_MODE="${DAEMON_MODE:-auto}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# WSL2-specific: Use the WSL kind config
KIND_CONFIG="${SCRIPT_DIR}/kind-config-wsl.yaml"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_wsl() { echo -e "${BLUE}[WSL2]${NC} $1"; }

# Check if running in WSL2
check_wsl2() {
    if grep -qi microsoft /proc/version 2>/dev/null; then
        log_wsl "Detected WSL2 environment."
        return 0
    else
        log_warn "Not running in WSL2. Consider using local-deploy.sh for native Linux."
        return 0  # Don't fail, just warn
    fi
}

# Check cgroups v2 configuration (critical for KIND on WSL2)
# See: https://kind.sigs.k8s.io/docs/user/known-issues/#failure-to-create-cluster-on-wsl2
check_cgroups() {
    if [[ -f /sys/fs/cgroup/cgroup.controllers ]]; then
        log_wsl "cgroups v2 detected and configured."
    else
        log_warn "cgroups v2 may not be properly configured."
        log_warn "If cluster creation fails with 'error adding pid to cgroups':"
        log_warn "  1. See KIND known issues: https://kind.sigs.k8s.io/docs/user/known-issues/"
        log_warn "  2. Fix with: https://github.com/spurin/wsl-cgroupsv2"
        log_warn ""
    fi
}

# Check prerequisites
check_prereqs() {
    log_info "Checking prerequisites..."

    # Check WSL2
    check_wsl2

    # Check cgroups v2 (critical for KIND)
    check_cgroups

    # Docker
    if ! command -v docker >/dev/null 2>&1; then
        log_error "docker is required but not installed."
        log_info "On WSL2, ensure Docker Desktop is installed and WSL2 integration is enabled."
        exit 1
    fi

    # Verify Docker is running
    if ! docker info >/dev/null 2>&1; then
        log_error "Docker is not running."
        log_info "On WSL2, start Docker Desktop and ensure 'Use the WSL 2 based engine' is enabled."
        exit 1
    fi

    # kind
    if ! command -v kind >/dev/null 2>&1; then
        log_error "kind is required but not installed."
        log_info "Install with: go install sigs.k8s.io/kind@latest"
        log_info "Or: curl -Lo ./kind https://kind.sigs.k8s.io/dl/latest/kind-linux-amd64 && chmod +x ./kind && sudo mv ./kind /usr/local/bin/"
        exit 1
    fi

    # kubectl
    if ! command -v kubectl >/dev/null 2>&1; then
        log_error "kubectl is required but not installed."
        exit 1
    fi

    # Check for WSL-specific kind config
    if [[ ! -f "${KIND_CONFIG}" ]]; then
        log_error "WSL2 kind config not found at ${KIND_CONFIG}"
        log_info "This script requires kind-config-wsl.yaml"
        exit 1
    fi

    log_info "All prerequisites satisfied."
}

# Create kind cluster
create_cluster() {
    log_info "Creating kind cluster '${CLUSTER_NAME}' (WSL2 mode)..."

    if kind get clusters | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Cluster '${CLUSTER_NAME}' already exists. Skipping creation."
        return 0
    fi

    log_wsl "Using WSL2-optimized kind configuration: ${KIND_CONFIG}"
    kind create cluster --name "${CLUSTER_NAME}" --config "${KIND_CONFIG}"

    # Wait for cluster to be ready (longer timeout for WSL2)
    log_info "Waiting for cluster to be ready (this may take longer on WSL2)..."
    kubectl wait --for=condition=Ready nodes --all --timeout=180s

    log_info "Cluster '${CLUSTER_NAME}' created successfully."
}

# Build container images
build_images() {
    log_info "Building container images (variant: ${IMAGE_VARIANT})..."

    cd "${ROOT_DIR}"

    # Argus Operator (needs repo root context for gen/go)
    log_info "Building argus-operator..."
    docker build -t "localhost/argus-operator:${IMAGE_TAG}" -f operators/argus-operator/Dockerfile .

    # Janus Operator (needs repo root context for gen/go)
    log_info "Building janus-operator..."
    docker build -t "localhost/janus-operator:${IMAGE_TAG}" -f operators/janus-operator/Dockerfile .

    # Build daemon images using unified Dockerfile.rust with --build-arg FEATURES
    # All variants use FROM scratch for minimal image size (~5-8 MB)
    case "${IMAGE_VARIANT}" in
        slim)
            log_wsl "Building slim images (~5-6 MB, FROM scratch, traditional mode only)"
            log_info "Building argusd (slim: traditional mode only - inotify)..."
            docker build --build-arg FEATURES= -t "localhost/argusd:slim" -f daemons/argusd/Dockerfile.rust .

            log_info "Building janusd (slim: traditional mode only - fanotify)..."
            docker build --build-arg FEATURES= -t "localhost/janusd:slim" -f daemons/janusd/Dockerfile.rust .
            ;;
        ebpf)
            log_warn "eBPF variant requested - requires custom WSL2 kernel with BPF LSM support"
            log_info "Building argusd (eBPF: forced eBPF mode, ~6-8 MB)..."
            docker build --build-arg FEATURES=ebpf -t "localhost/argusd:ebpf" -f daemons/argusd/Dockerfile.rust .

            log_info "Building janusd (eBPF: forced eBPF mode, ~6-8 MB)..."
            docker build --build-arg FEATURES=ebpf -t "localhost/janusd:ebpf" -f daemons/janusd/Dockerfile.rust .
            ;;
        full|*)
            log_wsl "Building full images (~6-8 MB, FROM scratch, runtime auto-detection)"
            log_info "Building argusd (full: runtime auto-detection - eBPF or inotify)..."
            docker build --build-arg FEATURES=ebpf -t "localhost/argusd:${IMAGE_TAG}" -f daemons/argusd/Dockerfile.rust .

            log_info "Building janusd (full: runtime auto-detection - eBPF or fanotify)..."
            docker build --build-arg FEATURES=ebpf -t "localhost/janusd:${IMAGE_TAG}" -f daemons/janusd/Dockerfile.rust .
            ;;
    esac

    # Guard-wait init container (for JanusGuard webhook injection)
    log_info "Building guard-wait..."
    docker build -t "localhost/guard-wait:${IMAGE_TAG}" -f tools/guard-wait/Dockerfile .

    # Watcher-wait init container (for ArgusWatcher webhook injection)
    log_info "Building watcher-wait..."
    docker build -t "localhost/watcher-wait:${IMAGE_TAG}" -f tools/watcher-wait/Dockerfile .

    # Panoptes Eye UI (needs repo root as context for proto/ access)
    log_info "Building panoptes-eye..."
    docker build -t "localhost/panoptes-eye:${IMAGE_TAG}" -f ui/panoptes-eye/Dockerfile .

    log_info "All images built successfully."
}

# Load images into kind
load_images() {
    log_info "Loading images into kind cluster (variant: ${IMAGE_VARIANT})..."

    kind load docker-image "localhost/argus-operator:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/janus-operator:${IMAGE_TAG}" --name "${CLUSTER_NAME}"

    # Load daemon images with correct tag based on variant
    case "${IMAGE_VARIANT}" in
        slim)
            kind load docker-image "localhost/argusd:slim" --name "${CLUSTER_NAME}"
            kind load docker-image "localhost/janusd:slim" --name "${CLUSTER_NAME}"
            ;;
        ebpf)
            kind load docker-image "localhost/argusd:ebpf" --name "${CLUSTER_NAME}"
            kind load docker-image "localhost/janusd:ebpf" --name "${CLUSTER_NAME}"
            ;;
        full|*)
            kind load docker-image "localhost/argusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
            kind load docker-image "localhost/janusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
            ;;
    esac

    kind load docker-image "localhost/guard-wait:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/watcher-wait:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/panoptes-eye:${IMAGE_TAG}" --name "${CLUSTER_NAME}"

    log_info "All images loaded into kind."
}

# Deploy Panoptes stack
deploy_stack() {
    log_info "Deploying Panoptes stack..."

    # Create namespace
    kubectl create namespace "${NAMESPACE}" --dry-run=client -o yaml | kubectl apply -f -

    # Install metrics-server for resource metrics (if not already installed)
    if ! kubectl get deployment metrics-server -n kube-system >/dev/null 2>&1; then
        log_info "Installing metrics-server..."
        kubectl apply -f https://github.com/kubernetes-sigs/metrics-server/releases/latest/download/components.yaml
        # Patch for KIND (insecure TLS needed for self-signed kubelet certs)
        kubectl patch deployment metrics-server -n kube-system --type='json' \
            -p='[{"op": "add", "path": "/spec/template/spec/containers/0/args/-", "value": "--kubelet-insecure-tls"}]'
    else
        log_info "metrics-server already installed."
    fi

    # Install CRDs
    log_info "Installing CRDs..."
    kubectl apply -f "${ROOT_DIR}/operators/argus-operator/config/crd/bases/"
    kubectl apply -f "${ROOT_DIR}/operators/janus-operator/config/crd/bases/"

    # Wait for CRDs to be established
    kubectl wait --for=condition=Established crd/arguswatchers.argus.como-technologies.io --timeout=30s
    kubectl wait --for=condition=Established crd/janusguards.janus.como-technologies.io --timeout=30s

    # Deploy operators using kustomize with local image override
    log_info "Deploying operators..."

    # Deploy argus-operator with local image override
    cd "${ROOT_DIR}/operators/argus-operator"
    kubectl kustomize config/default | \
        sed "s|controller:latest|localhost/argus-operator:${IMAGE_TAG}|g" | \
        sed "s|argusd:latest|localhost/argusd:${IMAGE_TAG}|g" | \
        sed "s|argus-operator-system|${NAMESPACE}|g" | \
        kubectl apply -f -

    # Deploy janus-operator with local image override
    cd "${ROOT_DIR}/operators/janus-operator"
    kubectl kustomize config/default | \
        sed "s|controller:latest|localhost/janus-operator:${IMAGE_TAG}|g" | \
        sed "s|janusd:latest|localhost/janusd:${IMAGE_TAG}|g" | \
        sed "s|janus-operator-system|${NAMESPACE}|g" | \
        kubectl apply -f -

    cd "${ROOT_DIR}"

    # NOTE: Webhook injection is disabled by default in local development.
    # The operators require --enable-webhook=true and TLS certificates to enable webhooks.
    # To enable webhooks, you would need to:
    # 1. Generate TLS certificates (via cert-manager or self-signed)
    # 2. Create a secret with the certs
    # 3. Mount the secret in the operator deployment
    # 4. Add --enable-webhook=true --webhook-cert-path=/certs to operator args
    # 5. Deploy the MutatingWebhookConfiguration from config/webhook/manifests.yaml
    #
    # For now, pods start without waiting for protection to be active.
    # The daemons will still monitor pods once they're running.

    # Deploy daemons as DaemonSets
    # Use variant tag for slim/ebpf, IMAGE_TAG for full
    log_info "Deploying daemons (variant: ${IMAGE_VARIANT})..."
    case "${IMAGE_VARIANT}" in
        slim)
            DAEMON_TAG="slim"
            ;;
        ebpf)
            DAEMON_TAG="ebpf"
            ;;
        full|*)
            DAEMON_TAG="${IMAGE_TAG}"
            ;;
    esac

    sed "s|localhost/argusd:dev|localhost/argusd:${DAEMON_TAG}|g" "${SCRIPT_DIR}/argusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -
    sed "s|localhost/janusd:dev|localhost/janusd:${DAEMON_TAG}|g" "${SCRIPT_DIR}/janusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -

    # Deploy Panoptes Eye UI
    log_info "Deploying Panoptes Eye UI..."
    kubectl apply -f "${SCRIPT_DIR}/panoptes-eye-local.yaml"

    # Wait for deployments (longer timeout for WSL2)
    log_info "Waiting for pods to be ready (WSL2 may be slower)..."
    kubectl wait --for=condition=Ready pods --all -n "${NAMESPACE}" --timeout=300s || true

    log_info "Stack deployed. Checking status..."
    kubectl get pods -n "${NAMESPACE}"
}

# Create test resources
create_test_resources() {
    log_info "Creating test resources..."

    # NOTE: Webhook injection labels are not needed when webhooks are disabled.
    # If webhooks are enabled, uncomment the following to enable injection:
    # kubectl label namespace default janus.panoptes.io/guard-injection=enabled --overwrite
    # kubectl label namespace default argus.panoptes.io/watcher-injection=enabled --overwrite

    # Test application
    kubectl apply -f - <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: test-app
  namespace: default
  labels:
    app: test-app
    security.panoptes.io/monitored: "true"
spec:
  containers:
  - name: alpine
    image: alpine:latest
    command: ["sleep", "infinity"]
EOF

    kubectl wait --for=condition=Ready pod/test-app -n default --timeout=60s

    # ArgusWatcher
    kubectl apply -f - <<EOF
apiVersion: argus.como-technologies.io/v1
kind: ArgusWatcher
metadata:
  name: test-watcher
  namespace: default
spec:
  selector:
    matchLabels:
      app: test-app
  subjects:
    - paths:
        - /etc/passwd
        - /etc/hosts
      events:
        - modify
        - create
        - delete
      tags:
        severity: high
EOF

    # JanusGuard
    kubectl apply -f - <<EOF
apiVersion: janus.como-technologies.io/v1
kind: JanusGuard
metadata:
  name: test-guard
  namespace: default
spec:
  selector:
    matchLabels:
      app: test-app
  enforcing: true
  subjects:
    - deny:
        - /etc/shadow
      events:
        - access
      audit: true
EOF

    log_info "Test resources created."
}

# Run test scenario
run_test() {
    log_info "Running test scenario..."

    # Trigger file modification
    log_info "Triggering file modification..."
    kubectl exec test-app -- sh -c "echo '# test' >> /etc/hosts"

    sleep 2

    # Check for events
    log_info "Checking ArgusWatcher status..."
    kubectl get arguswatcher test-watcher -o yaml | grep -A5 status || true

    log_info "Checking JanusGuard status..."
    kubectl get janusguard test-guard -o yaml | grep -A5 status || true

    log_info "Recent operator logs:"
    kubectl logs -n "${NAMESPACE}" -l app.kubernetes.io/name=argus-operator --tail=10 || true

    log_info "Test complete. Access UI at http://localhost:3000"
}

# Show port mappings (kind handles this via extraPortMappings in kind-config-wsl.yaml)
start_port_forwards() {
    log_info "Port mappings (via kind NodePort):"
    log_info "  - Panoptes Eye: http://localhost:3000 (NodePort 30080)"
    log_info "  - Argus metrics: http://localhost:8080 (NodePort 30081)"
    log_info "  - Janus metrics: http://localhost:8081 (NodePort 30082)"
    log_wsl "Access these URLs directly in your Windows browser"
    log_wsl "No port-forward needed - kind extraPortMappings handles this"
}

# Update daemon DaemonSets with correct image variant
# This reapplies manifests with the correct image tag based on IMAGE_VARIANT
update_daemon_images() {
    log_info "Updating daemon images (variant: ${IMAGE_VARIANT})..."

    # Determine daemon tag based on variant
    case "${IMAGE_VARIANT}" in
        slim)
            DAEMON_TAG="slim"
            ;;
        ebpf)
            DAEMON_TAG="ebpf"
            ;;
        full|*)
            DAEMON_TAG="${IMAGE_TAG}"
            ;;
    esac

    log_info "Using daemon image tag: ${DAEMON_TAG}"

    # Update argusd DaemonSet
    sed "s|localhost/argusd:dev|localhost/argusd:${DAEMON_TAG}|g" "${SCRIPT_DIR}/argusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -

    # Update janusd DaemonSet
    sed "s|localhost/janusd:dev|localhost/janusd:${DAEMON_TAG}|g" "${SCRIPT_DIR}/janusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -
}

# Force restart all Panoptes pods to pick up new images
# This is needed because imagePullPolicy:Never + static tag means
# kubectl apply won't restart pods when images are updated
restart_pods() {
    log_info "Restarting pods to pick up new images..."

    # Restart deployments
    kubectl rollout restart deployment/panoptes-eye -n "${NAMESPACE}" 2>/dev/null || true
    kubectl rollout restart deployment/argus-operator-controller-manager -n "${NAMESPACE}" 2>/dev/null || true
    kubectl rollout restart deployment/janus-operator-controller-manager -n "${NAMESPACE}" 2>/dev/null || true

    # Restart DaemonSets (argusd, janusd)
    kubectl rollout restart daemonset/argusd -n "${NAMESPACE}" 2>/dev/null || true
    kubectl rollout restart daemonset/janusd -n "${NAMESPACE}" 2>/dev/null || true

    # Wait for rollouts to complete
    log_info "Waiting for rollouts to complete..."
    kubectl rollout status deployment/panoptes-eye -n "${NAMESPACE}" --timeout=120s 2>/dev/null || true
    kubectl rollout status deployment/argus-operator-controller-manager -n "${NAMESPACE}" --timeout=120s 2>/dev/null || true
    kubectl rollout status deployment/janus-operator-controller-manager -n "${NAMESPACE}" --timeout=120s 2>/dev/null || true
    kubectl rollout status daemonset/argusd -n "${NAMESPACE}" --timeout=120s 2>/dev/null || true
    kubectl rollout status daemonset/janusd -n "${NAMESPACE}" --timeout=120s 2>/dev/null || true

    log_info "Pods restarted. Checking status..."
    kubectl get pods -n "${NAMESPACE}"
}

# Clean up everything
cleanup() {
    log_info "Cleaning up..."

    # Delete test resources
    kubectl delete arguswatcher test-watcher --ignore-not-found || true
    kubectl delete janusguard test-guard --ignore-not-found || true
    kubectl delete pod test-app --ignore-not-found || true

    # Delete cluster
    kind delete cluster --name "${CLUSTER_NAME}" || true

    log_info "Cleanup complete."
}

# Show usage
usage() {
    cat <<EOF
Panoptes Local Development Script (WSL2 Variant)

This script is optimized for Windows Subsystem for Linux 2. For native Linux,
use local-deploy.sh instead.

Usage: $0 [command]

Commands:
    cluster     Create kind cluster only (uses kind-config-wsl.yaml)
    build       Build container images
    load        Load images into kind cluster
    deploy      Deploy Panoptes stack
    test        Create test resources and run test
    forward     Show port mappings
    restart     Restart all pods to pick up new images
    redeploy    Quick iteration: build + load + update manifests + restart
    clean       Delete cluster and resources
    all         Run full setup (cluster + build + load + deploy + test)

Environment Variables:
    CLUSTER_NAME    Kind cluster name (default: panoptes-dev)
    NAMESPACE       Kubernetes namespace (default: panoptes-system)
    IMAGE_TAG       Container image tag (default: dev)
    IMAGE_VARIANT   Image variant to build (default: slim for WSL2)
                    slim = Traditional only, no eBPF (~50MB, faster build)
                    full = Both modes + runtime auto-detection (~87MB)
                    ebpf = eBPF forced mode (~87MB, needs custom kernel)
    DAEMON_MODE     Runtime mode override for full variant (default: auto)
                    auto        = auto-detect at runtime (eBPF if supported)
                    ebpf        = force eBPF mode
                    traditional = force traditional mode

WSL2 Notes:
    - Docker Desktop must be running with WSL2 integration enabled
    - Uses pinned K8s 1.30.0 for stability
    - Extended timeouts for slower WSL2 performance
    - Access UI at http://localhost:3000 from Windows browser
    - Default IMAGE_VARIANT=slim (smaller images, faster builds)
    - eBPF mode requires a WSL2 kernel with BPF LSM support (custom kernel)
    - Use IMAGE_VARIANT=full to enable auto-detection with fallback

Runtime Mode Selection (auto-detect with fallback):
    The daemons automatically detect kernel capabilities at startup:
    - If eBPF is supported (kernel 5.8+ with BTF): use eBPF mode
    - Otherwise: fallback to traditional mode (inotify/fanotify)

    eBPF mode features:
    - Argus: LSM-based monitoring with full process attribution
    - Janus: LSM-based auditing with atomic process info + deny rules

    Traditional mode features:
    - Argus: inotify-based file monitoring (no process info)
    - Janus: fanotify-based access auditing (process info via /proc)

Examples:
    $0 all                              # Full setup (slim variant, faster)
    IMAGE_VARIANT=full $0 all           # Full images with auto-detection
    $0 redeploy                         # Quick iteration: rebuild + reload + restart
    $0 build && $0 load                 # Rebuild and reload images only
    $0 restart                          # Restart pods to pick up loaded images
    IMAGE_TAG=v2.0.0 $0 build           # Build with specific tag
EOF
}

# Main
main() {
    check_prereqs

    case "${1:-all}" in
        cluster)
            create_cluster
            ;;
        build)
            build_images
            ;;
        load)
            load_images
            ;;
        deploy)
            deploy_stack
            ;;
        test)
            create_test_resources
            run_test
            ;;
        forward)
            start_port_forwards
            ;;
        restart)
            restart_pods
            ;;
        redeploy)
            build_images
            load_images
            update_daemon_images
            restart_pods
            start_port_forwards
            ;;
        clean)
            cleanup
            ;;
        all)
            create_cluster
            build_images
            load_images
            deploy_stack
            create_test_resources
            start_port_forwards
            run_test
            ;;
        -h|--help|help)
            usage
            ;;
        *)
            log_error "Unknown command: $1"
            usage
            exit 1
            ;;
    esac
}

main "$@"
