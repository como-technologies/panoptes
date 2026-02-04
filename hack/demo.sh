#!/bin/bash
# Panoptes Demo Script
# One-command demo: creates a Kind cluster, deploys Panoptes, applies sample watchers,
# triggers sample events, and opens the dashboard.
#
# Usage: ./hack/demo.sh
#
# Copyright 2026 Como Technologies, LTD
# Licensed under Apache License 2.0

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CLUSTER_NAME="${CLUSTER_NAME:-panoptes-demo}"
NAMESPACE="panoptes-system"
IMAGE_TAG="dev"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC}  $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "\n${BLUE}${BOLD}==>${NC} ${BOLD}$1${NC}"; }

# Cleanup on exit (optional)
cleanup() {
    if [ "${KEEP_CLUSTER:-false}" != "true" ]; then
        echo ""
        read -r -p "Delete the demo cluster? [y/N] " response
        if [[ "$response" =~ ^[Yy]$ ]]; then
            kind delete cluster --name "${CLUSTER_NAME}" 2>/dev/null || true
            log_info "Demo cluster deleted."
        else
            log_info "Cluster '${CLUSTER_NAME}' kept. Delete later with: kind delete cluster --name ${CLUSTER_NAME}"
        fi
    fi
}

# Check prerequisites
check_prereqs() {
    log_step "Checking prerequisites"
    local missing=()

    command -v docker >/dev/null 2>&1 || missing+=("docker")
    command -v kind >/dev/null 2>&1 || missing+=("kind")
    command -v kubectl >/dev/null 2>&1 || missing+=("kubectl")

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        echo "Install them and try again."
        exit 1
    fi

    # Check Docker is running
    docker info >/dev/null 2>&1 || {
        log_error "Docker daemon is not running."
        exit 1
    }

    log_info "All prerequisites satisfied."
}

# Create Kind cluster
create_cluster() {
    log_step "Creating Kind cluster '${CLUSTER_NAME}'"

    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Cluster already exists, reusing it."
        kubectl cluster-info --context "kind-${CLUSTER_NAME}" >/dev/null 2>&1 || {
            log_error "Cluster exists but is not reachable. Delete it with: kind delete cluster --name ${CLUSTER_NAME}"
            exit 1
        }
        return 0
    fi

    kind create cluster --name "${CLUSTER_NAME}" --wait 60s --config - <<EOF
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
  - role: control-plane
    extraPortMappings:
      - containerPort: 30080
        hostPort: 3000
        protocol: TCP
containerdConfigPatches:
  - |-
    [plugins."io.containerd.grpc.v1.cri".containerd]
      discard_unpacked_layers = false
EOF

    log_info "Cluster created successfully."
}

# Build and load images
build_images() {
    log_step "Building Docker images"

    cd "${ROOT_DIR}"

    log_info "Building argus-operator..."
    docker build -t "argus-controller:${IMAGE_TAG}" -f operators/argus-operator/Dockerfile . 2>&1 | tail -1

    log_info "Building janus-operator..."
    docker build -t "janus-controller:${IMAGE_TAG}" -f operators/janus-operator/Dockerfile . 2>&1 | tail -1

    log_info "Building argusd..."
    docker build -t "argusd:${IMAGE_TAG}" -f daemons/argusd/Dockerfile . 2>&1 | tail -1

    log_info "Building janusd..."
    docker build -t "janusd:${IMAGE_TAG}" -f daemons/janusd/Dockerfile . 2>&1 | tail -1

    log_step "Loading images into Kind cluster"
    kind load docker-image "argus-controller:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "janus-controller:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "argusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"
    kind load docker-image "janusd:${IMAGE_TAG}" --name "${CLUSTER_NAME}"

    log_info "All images built and loaded."
}

# Deploy Panoptes
deploy_stack() {
    log_step "Deploying Panoptes stack"

    # Create namespace
    kubectl create namespace "${NAMESPACE}" --dry-run=client -o yaml | kubectl apply -f -

    # Label namespace for privileged pods
    kubectl label namespace "${NAMESPACE}" \
        pod-security.kubernetes.io/enforce=privileged \
        pod-security.kubernetes.io/warn=privileged \
        --overwrite

    # Deploy using the local-deploy script
    "${SCRIPT_DIR}/local-deploy.sh" deploy

    # Wait for all pods to be ready
    log_info "Waiting for pods to be ready..."
    kubectl wait --for=condition=ready pod -l app.kubernetes.io/part-of=panoptes \
        -n "${NAMESPACE}" --timeout=120s 2>/dev/null || {
        log_warn "Some pods are not ready yet. Continuing anyway..."
        kubectl get pods -n "${NAMESPACE}"
    }

    log_info "Panoptes stack deployed."
}

# Apply sample watchers and guards
apply_samples() {
    log_step "Applying sample ArgusWatcher and JanusGuard"

    # Create a test namespace with labeled pods
    kubectl create namespace demo --dry-run=client -o yaml | kubectl apply -f -

    # Deploy a test workload
    kubectl apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: demo-app
  namespace: demo
  labels:
    app: demo-app
    panoptes.como-technologies.io/monitored: "true"
spec:
  replicas: 1
  selector:
    matchLabels:
      app: demo-app
  template:
    metadata:
      labels:
        app: demo-app
        panoptes.como-technologies.io/monitored: "true"
    spec:
      containers:
        - name: app
          image: busybox:1.36
          command: ["sleep", "infinity"]
EOF

    log_info "Waiting for demo-app pod..."
    kubectl wait --for=condition=ready pod -l app=demo-app -n demo --timeout=60s

    # Apply ArgusWatcher for FIM
    kubectl apply -f - <<EOF
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: demo-watcher
  namespace: demo
spec:
  selector:
    matchLabels:
      app: demo-app
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
        - /etc/crontab
        - /tmp
      events:
        - create
        - modify
        - delete
        - moved_to
      recursive: false
      tags:
        demo: "true"
        severity: high
EOF

    # Apply JanusGuard for access auditing
    kubectl apply -f - <<EOF
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: demo-guard
  namespace: demo
spec:
  enforcing: false
  selector:
    matchLabels:
      app: demo-app
  subjects:
    - deny:
        - /etc/shadow
        - /var/run/docker.sock
      events:
        - open
        - access
      defaultResponse: audit
      tags:
        demo: "true"
        compliance: security-baseline
EOF

    log_info "Sample watchers and guards applied."
}

# Generate sample events
trigger_events() {
    log_step "Triggering sample file events"

    local pod
    pod=$(kubectl get pod -n demo -l app=demo-app -o jsonpath='{.items[0].metadata.name}')

    log_info "Creating test files in demo pod..."
    kubectl exec -n demo "${pod}" -- sh -c '
        echo "test" > /tmp/test-file.txt
        echo "modified" >> /tmp/test-file.txt
        cat /etc/passwd > /dev/null
        cat /etc/shadow 2>/dev/null || true
        rm /tmp/test-file.txt
    ' 2>/dev/null || true

    log_info "Sample events triggered. These should appear in the event stream."
}

# Print summary
print_summary() {
    log_step "Demo Ready"

    echo ""
    echo -e "${BOLD}Panoptes Security Suite Demo${NC}"
    echo "============================================"
    echo ""
    echo "Components deployed:"
    kubectl get pods -n "${NAMESPACE}" --no-headers 2>/dev/null | while read -r line; do
        echo "  - ${line}"
    done
    echo ""
    echo "Demo resources:"
    echo "  - Namespace: demo"
    echo "  - ArgusWatcher: demo-watcher (monitoring /etc/passwd, /etc/shadow, /etc/crontab, /tmp)"
    echo "  - JanusGuard: demo-guard (auditing access to /etc/shadow, docker.sock)"
    echo ""
    echo "Check status:"
    echo "  kubectl get aw,jg -A"
    echo "  kubectl describe aw demo-watcher -n demo"
    echo "  kubectl describe jg demo-guard -n demo"
    echo ""
    echo "View daemon logs:"
    echo "  kubectl logs -n ${NAMESPACE} -l app.kubernetes.io/component=daemon -f"
    echo ""
    echo "Trigger more events:"
    local pod
    pod=$(kubectl get pod -n demo -l app=demo-app -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "demo-app-xxx")
    echo "  kubectl exec -n demo ${pod} -- sh -c 'echo test > /tmp/hello.txt'"
    echo ""
    echo "Cleanup:"
    echo "  kind delete cluster --name ${CLUSTER_NAME}"
    echo ""
}

# Main
main() {
    echo -e "${BOLD}"
    echo "  ____                         _            "
    echo " |  _ \ __ _ _ __   ___  _ __ | |_ ___  ___ "
    echo " | |_) / _\` | '_ \ / _ \| '_ \| __/ _ \/ __|"
    echo " |  __/ (_| | | | | (_) | |_) | ||  __/\__ \\"
    echo " |_|   \__,_|_| |_|\___/| .__/ \__\___||___/"
    echo "                        |_|    Demo Launcher "
    echo -e "${NC}"

    trap cleanup EXIT

    check_prereqs
    create_cluster
    build_images
    deploy_stack
    apply_samples
    trigger_events
    print_summary
}

main "$@"
