# Panoptes Quick Start

**Panoptes** is a Kubernetes-native file integrity monitoring (FIM) and file access auditing system:
- **Argus**: FIM using Linux inotify (ArgusWatcher CRD, short name: `aw`)
- **Janus**: Access auditing/enforcement using Linux fanotify (JanusGuard CRD, short name: `jg`)
- **Panoptes Eye**: Real-time web dashboard

**API groups**: `argus.como-technologies.io/v2`, `janus.como-technologies.io/v2`

---

## 1. See It in 60 Seconds

If you already have a Kubernetes cluster with Panoptes deployed, you can create a watcher in seconds:

```bash
# Create a simple watcher
kubectl apply -f - <<EOF
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: quick-test
  namespace: default
spec:
  selector:
    matchLabels:
      app: nginx
  subjects:
    - paths: ["/etc/passwd", "/etc/shadow"]
      events: [create, modify, delete]
      tags:
        severity: critical
EOF

# Check status
kubectl get aw

# See events (if Panoptes Eye is deployed)
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

**Don't have Panoptes deployed yet?** Follow the full setup below.

---

## 2. Prerequisites

### Option A: Dev Container (Recommended)

The dev container ships every tool pre-installed. The only host requirement is a
container runtime — **Docker or Podman** (either works).

```bash
# 1. Clone the repo
git clone https://github.com/como-technologies/panoptes.git
cd panoptes

# 2. Open in your IDE (VS Code, JetBrains, or CLI)
#    VS Code:  "Dev Containers: Reopen in Container" from the command palette
#    CLI:      devcontainer up --workspace-folder .

# 3. Once inside the container, all tools are on PATH — skip to step 3 below.
```

Works on Linux, macOS, Windows/WSL2, and GitHub Codespaces.

### Option B: Native Install

If you prefer not to use the dev container, install these on your host:

```bash
# Docker
docker --version  # 20.10+

# kind (Kubernetes in Docker)
kind --version    # 0.20+

# kubectl
kubectl version --client  # 1.28+
```

Install kind: https://kind.sigs.k8s.io/docs/user/quick-start/#installation

---

## 3. One-Command Setup

```bash
# If you used the dev container, you're already in the repo.
# Otherwise, clone first:
git clone https://github.com/como-technologies/panoptes.git
cd panoptes

# Deploy everything (creates Kind cluster, builds images, deploys stack)
./hack/local-deploy.sh all
```

**For WSL2 users (native install only):** Use `./hack/local-wsl-deploy.sh all` instead.

This command:
1. Creates a Kind cluster named `panoptes-dev`
2. Builds all container images (operators, daemons, UI)
3. Loads images into Kind
4. Deploys CRDs, operators, daemons, and dashboard

Takes ~5-7 minutes on first run (image builds are cached).

---

## 4. What You'll See

After deployment, you'll have the `panoptes-system` namespace with:

```bash
kubectl get pods -n panoptes-system
```

Expected output:
```
NAME                                READY   STATUS    RESTARTS   AGE
argus-operator-6d9b8c7f5-xxxxx     1/1     Running   0          2m
argusd-xxxxx                        1/1     Running   0          2m
janus-operator-7c8d9e6f4-xxxxx     1/1     Running   0          2m
janusd-xxxxx                        1/1     Running   0          2m
panoptes-eye-5f4e3d2c1-xxxxx       1/1     Running   0          2m
```

Check for active watchers and guards (should be empty initially):

```bash
kubectl get aw,jg -A
# No resources found
```

---

## 5. Create Your First Watcher

Deploy a test workload and monitor it:

```bash
# Deploy nginx test pod
kubectl run nginx --image=nginx --labels="app=nginx"

# Wait for pod
kubectl wait --for=condition=Ready pod/nginx --timeout=60s

# Apply watcher from samples
kubectl apply -f operators/argus-operator/config/samples/argus_v2_arguswatcher_minimal.yaml

# Verify watcher is active
kubectl get aw
```

Expected output:
```
NAME            AGE
basic-watcher   5s
```

Trigger a file event:

```bash
# Modify watched file
kubectl exec nginx -- sh -c "echo test >> /app/data"

# Check watcher status for events
kubectl get aw basic-watcher -o yaml
```

**Note:** The minimal sample watches `/app/data` for the `myapp` label. To watch nginx specifically, create a custom watcher:

```bash
kubectl apply -f - <<EOF
apiVersion: argus.como-technologies.io/v2
kind: ArgusWatcher
metadata:
  name: nginx-watcher
  namespace: default
spec:
  selector:
    matchLabels:
      run: nginx
  subjects:
    - paths:
        - /etc/passwd
        - /etc/shadow
      events:
        - create
        - modify
        - delete
      tags:
        severity: critical
EOF

# Trigger an event
kubectl exec nginx -- sh -c "echo '# test' >> /etc/passwd"

# View events in operator logs
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argus-operator --tail=20
```

---

## 6. Try Access Control (Optional)

Deploy a JanusGuard to audit and block access to sensitive files:

```bash
kubectl apply -f - <<EOF
apiVersion: janus.como-technologies.io/v2
kind: JanusGuard
metadata:
  name: block-shadow
  namespace: default
spec:
  selector:
    matchLabels:
      run: nginx
  subjects:
    - deny:
        - /etc/shadow
      events:
        - access
        - open
      audit: true
      enforcing: false  # Dry-run mode (logs only)
      tags:
        severity: critical
        compliance: pci-dss
EOF

# Test access (will be audited but not blocked in dry-run mode)
kubectl exec nginx -- cat /etc/shadow

# View audit logs
kubectl logs -n panoptes-system -l app.kubernetes.io/name=janus-operator --tail=20

# Enable enforcement (blocks access)
kubectl patch jg block-shadow -p '{"spec":{"subjects":[{"deny":["/etc/shadow"],"events":["access","open"],"audit":true,"enforcing":true,"tags":{"severity":"critical","compliance":"pci-dss"}}]}}'

# Test again (should fail with permission denied)
kubectl exec nginx -- cat /etc/shadow
```

---

## 7. Open the Dashboard

Access Panoptes Eye for real-time visibility:

```bash
# Port-forward the UI service
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

Open http://localhost:3000 in your browser.

Dashboard features:
- **Watchers**: View all ArgusWatcher resources across namespaces
- **Guards**: View all JanusGuard resources and enforcement status
- **Events**: Real-time file event stream from both Argus and Janus
- **Explorer**: Browse container filesystems (coming soon)

---

## 8. Next Steps

**Use-case quickstarts:**
- Security monitoring: `guides/quick-start-security.md`
- Compliance by framework: `guides/monitoring-by-compliance.md`
- Multi-cluster setup: `guides/multi-cluster.md`
- Enabling enforcement: `guides/enabling-enforcement.md`

**Deployment guides:**
- Spectro Cloud Palette: `docs/SPECTRO_QUICK_START.md`
- Webhook injection for pre-app protection: `docs/WEBHOOK_DEPLOYMENT.md`

**Best practices:**
- What to monitor: `guides/what-to-monitor.md`
- Sample gallery: `operators/argus-operator/config/samples/` and `operators/janus-operator/config/samples/`

**Metrics and observability:**
```bash
# Argus operator metrics
kubectl port-forward -n panoptes-system svc/argus-operator-metrics 8080:8080
curl localhost:8080/metrics | grep argus_

# Janus operator metrics
kubectl port-forward -n panoptes-system svc/janus-operator-metrics 8081:8080
curl localhost:8081/metrics | grep janus_
```

---

## 9. Clean Up

```bash
# Delete test resources
kubectl delete aw --all
kubectl delete jg --all
kubectl delete pod nginx

# Delete entire cluster
./hack/local-deploy.sh clean
```

---

## 10. Coming Soon

Pre-built container images will be available on GHCR soon, enabling one-command `helm install` without building from source.

---

## Troubleshooting

### Pods stuck in Pending
```bash
kubectl describe pod -n panoptes-system <pod-name>

# Common fixes:
# - Image not loaded: kind load docker-image <image> --name panoptes-dev
# - Insufficient memory: Increase Docker to 4GB+
```

### Daemons failing to start
```bash
# Daemons require privileged access
kubectl get pod -n panoptes-system argusd-xxx -o yaml | grep -A5 securityContext

# Required capabilities: SYS_ADMIN, SYS_PTRACE, DAC_READ_SEARCH
```

### No events detected
```bash
# Verify watcher targets the correct pods
kubectl get aw <name> -o jsonpath='{.spec.selector}'
kubectl get pods -l <selector-labels>

# Check daemon connectivity
kubectl logs -n panoptes-system argusd-xxx | grep -i grpc
```

---

## Quick Reference

| Command | Description |
|---------|-------------|
| `./hack/local-deploy.sh all` | Full automated setup |
| `./hack/local-deploy.sh build` | Build all images |
| `./hack/local-deploy.sh deploy` | Deploy to cluster |
| `./hack/local-deploy.sh clean` | Delete cluster |
| `kubectl get aw` | List ArgusWatchers |
| `kubectl get jg` | List JanusGuards |

**WSL2 users:** Use `./hack/local-wsl-deploy.sh` instead.

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
