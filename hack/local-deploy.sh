#!/bin/bash
# Panoptes Local Development Deployment Script
# Usage: ./hack/local-deploy.sh [build|deploy|test|clean|all]
#
# Copyright 2026 Como Technologies, LTD
# Licensed under Apache License 2.0

set -euo pipefail

# Configuration
CLUSTER_NAME="${CLUSTER_NAME:-panoptes-dev}"
NAMESPACE="${NAMESPACE:-panoptes-system}"
IMAGE_TAG="${IMAGE_TAG:-dev}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check prerequisites
check_prereqs() {
    log_info "Checking prerequisites..."

    command -v docker >/dev/null 2>&1 || { log_error "docker is required but not installed."; exit 1; }
    command -v kind >/dev/null 2>&1 || { log_error "kind is required but not installed."; exit 1; }
    command -v kubectl >/dev/null 2>&1 || { log_error "kubectl is required but not installed."; exit 1; }

    log_info "All prerequisites satisfied."
}

# Create kind cluster
create_cluster() {
    log_info "Creating kind cluster '${CLUSTER_NAME}'..."

    if kind get clusters | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Cluster '${CLUSTER_NAME}' already exists. Skipping creation."
        return 0
    fi

    kind create cluster --name "${CLUSTER_NAME}" --config "${SCRIPT_DIR}/kind-config.yaml"

    # Wait for cluster to be ready
    kubectl wait --for=condition=Ready nodes --all --timeout=120s

    log_info "Cluster '${CLUSTER_NAME}' created successfully."
}

# Build the grpc-static-builder base image (one-time, reusable)
build_grpc_builder() {
    log_info "Building grpc-static-builder base image..."
    log_warn "This will take 15-30 minutes but only needs to be done once."

    cd "${ROOT_DIR}"
    docker build -t "grpc-static-builder:1.60.0" -f hack/Dockerfile.grpc-static .

    log_info "grpc-static-builder:1.60.0 built successfully."
    log_info "You can now use 'build-fast' for quick daemon rebuilds."
}

# Build container images (full build, includes gRPC compilation)
build_images() {
    log_info "Building container images..."

    cd "${ROOT_DIR}"

    # Argus Operator (needs repo root context for gen/go)
    log_info "Building argus-operator..."
    docker build -t "localhost/argus-operator:${IMAGE_TAG}" -f operators/argus-operator/Dockerfile .

    # Janus Operator (needs repo root context for gen/go)
    log_info "Building janus-operator..."
    docker build -t "localhost/janus-operator:${IMAGE_TAG}" -f operators/janus-operator/Dockerfile .

    # Argusd daemon (needs repo root as context for proto/ access)
    # Note: First build takes 15-30 min (builds gRPC from source)
    log_info "Building argusd (static binary from scratch)..."
    docker build -t "localhost/argusd:${IMAGE_TAG}" -f daemons/argusd/Dockerfile .

    # Janusd daemon (needs repo root as context for proto/ access)
    log_info "Building janusd (static binary from scratch)..."
    docker build -t "localhost/janusd:${IMAGE_TAG}" -f daemons/janusd/Dockerfile .

    # Panoptes Eye UI (needs repo root as context for proto/ access)
    log_info "Building panoptes-eye..."
    docker build -t "localhost/panoptes-eye:${IMAGE_TAG}" -f ui/panoptes-eye/Dockerfile .

    log_info "All images built successfully."
}

# Fast build using pre-built grpc-static-builder image
build_images_fast() {
    log_info "Building container images (fast mode using cached gRPC builder)..."

    cd "${ROOT_DIR}"

    # Check if grpc-static-builder exists
    if ! docker image inspect grpc-static-builder:1.60.0 >/dev/null 2>&1; then
        log_error "grpc-static-builder:1.60.0 not found. Run 'build-grpc' first."
        exit 1
    fi

    # Argus Operator (needs repo root context for gen/go)
    log_info "Building argus-operator..."
    docker build -t "localhost/argus-operator:${IMAGE_TAG}" -f operators/argus-operator/Dockerfile .

    # Janus Operator (needs repo root context for gen/go)
    log_info "Building janus-operator..."
    docker build -t "localhost/janus-operator:${IMAGE_TAG}" -f operators/janus-operator/Dockerfile .

    # Argusd daemon using pre-built gRPC
    log_info "Building argusd (using cached grpc-static-builder)..."
    docker build -t "localhost/argusd:${IMAGE_TAG}" \
        --build-arg BUILDKIT_INLINE_CACHE=1 \
        -f daemons/argusd/Dockerfile .

    # Janusd daemon using pre-built gRPC
    log_info "Building janusd (using cached grpc-static-builder)..."
    docker build -t "localhost/janusd:${IMAGE_TAG}" \
        --build-arg BUILDKIT_INLINE_CACHE=1 \
        -f daemons/janusd/Dockerfile .

    # Panoptes Eye UI (needs repo root as context for proto/ access)
    log_info "Building panoptes-eye..."
    docker build -t "localhost/panoptes-eye:${IMAGE_TAG}" -f ui/panoptes-eye/Dockerfile .

    log_info "All images built successfully."
}

# Load images into kind
load_images() {
    log_info "Loading images into kind cluster..."

    kind load docker-image "localhost/argus-operator:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/janus-operator:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/argusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "localhost/janusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
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

    # Deploy daemons as DaemonSets
    log_info "Deploying daemons..."
    sed "s|localhost/argusd:dev|localhost/argusd:${IMAGE_TAG}|g" "${SCRIPT_DIR}/argusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -
    sed "s|localhost/janusd:dev|localhost/janusd:${IMAGE_TAG}|g" "${SCRIPT_DIR}/janusd-daemonset.yaml" | \
        sed "s|namespace: panoptes-system|namespace: ${NAMESPACE}|g" | \
        kubectl apply -f -

    # Deploy Panoptes Eye UI
    log_info "Deploying Panoptes Eye UI..."
    kubectl apply -f "${SCRIPT_DIR}/panoptes-eye-local.yaml"

    # Wait for deployments
    log_info "Waiting for pods to be ready..."
    kubectl wait --for=condition=Ready pods --all -n "${NAMESPACE}" --timeout=180s || true

    log_info "Stack deployed. Checking status..."
    kubectl get pods -n "${NAMESPACE}"
}

# Create test resources
create_test_resources() {
    log_info "Creating test resources..."

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
  enforcing: false
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

# Start port-forwards
start_port_forwards() {
    log_info "Starting port-forwards..."

    # Kill any existing port-forwards
    pkill -f "kubectl port-forward.*panoptes" || true

    # Panoptes Eye UI
    kubectl port-forward -n "${NAMESPACE}" svc/panoptes-eye 3000:3000 &

    log_info "Port-forwards started:"
    log_info "  - Panoptes Eye: http://localhost:3000"
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
Panoptes Local Development Script

Usage: $0 [command]

Commands:
    cluster     Create kind cluster only
    build       Build container images (full build, includes gRPC from source)
    build-grpc  Build grpc-static-builder base image (one-time, ~20 min)
    build-fast  Build using pre-built grpc-static-builder (requires build-grpc first)
    load        Load images into kind cluster
    deploy      Deploy Panoptes stack
    test        Create test resources and run test
    forward     Start port-forwards
    restart     Restart all pods to pick up new images
    redeploy    Quick iteration: build + load + restart pods
    clean       Delete cluster and resources
    all         Run full setup (cluster + build + load + deploy + test)

Environment Variables:
    CLUSTER_NAME    Kind cluster name (default: panoptes-dev)
    NAMESPACE       Kubernetes namespace (default: panoptes-system)
    IMAGE_TAG       Container image tag (default: dev)

Examples:
    $0 all                          # Full setup (first time, takes 20-30 min)
    $0 build-grpc                   # Build gRPC builder image once
    $0 build-fast && $0 load        # Fast rebuild and reload (after build-grpc)
    IMAGE_TAG=v2.0.0 $0 build       # Build with specific tag

Note: First build takes 15-30 minutes to compile gRPC from source.
      Use 'build-grpc' once, then 'build-fast' for quick iterations.
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
        build-grpc)
            build_grpc_builder
            ;;
        build-fast)
            build_images_fast
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
