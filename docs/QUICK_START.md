# Panoptes Quick Start: Local Kubernetes Testing

Get Panoptes running locally in 5 minutes. No fluff. Just commands.

---

## Prerequisites

```bash
# Docker (or Podman)
docker --version  # 20.10+

# kind (Kubernetes in Docker)
# Install: https://kind.sigs.k8s.io/docs/user/quick-start/#installation
kind --version    # 0.20+

# kubectl
kubectl version --client  # 1.28+

# Optional: Helm
helm version      # 3.12+
```

---

## 1. Create Local Cluster

```bash
cd /path/to/panoptes

# Create kind cluster with privileged container support
kind create cluster --name panoptes-dev --config hack/kind-config.yaml

# Verify
kubectl cluster-info --context kind-panoptes-dev
```

---

## 2. Build Container Images

```bash
# Build all images and load into kind (takes ~5 min first time)
./hack/local-deploy.sh build

# Or build individually:
# Operators
docker build -t localhost/argus-operator:dev operators/argus-operator/
docker build -t localhost/janus-operator:dev operators/janus-operator/

# Daemons
docker build -t localhost/argusd:dev daemons/argusd/
docker build -t localhost/janusd:dev daemons/janusd/

# UI
docker build -t localhost/panoptes-eye:dev ui/panoptes-eye/

# Load into kind
kind load docker-image localhost/argus-operator:dev --name panoptes-dev
kind load docker-image localhost/janus-operator:dev --name panoptes-dev
kind load docker-image localhost/argusd:dev --name panoptes-dev
kind load docker-image localhost/janusd:dev --name panoptes-dev
kind load docker-image localhost/panoptes-eye:dev --name panoptes-dev
```

---

## 3. Deploy Panoptes Stack

```bash
# One command deployment
./hack/local-deploy.sh deploy

# Or step-by-step:

# Create namespace
kubectl create namespace panoptes-system

# Install CRDs
kubectl apply -f operators/argus-operator/config/crd/bases/
kubectl apply -f operators/janus-operator/config/crd/bases/

# Deploy operators (using kustomize)
kubectl apply -k operators/argus-operator/config/default/
kubectl apply -k operators/janus-operator/config/default/

# Deploy Panoptes Eye UI
kubectl apply -f hack/panoptes-eye-local.yaml

# Verify pods are running
kubectl get pods -n panoptes-system -w
```

Expected output:
```
NAME                                READY   STATUS    RESTARTS   AGE
argus-operator-6d9b8c7f5-xxxxx     1/1     Running   0          30s
argusd-xxxxx                        1/1     Running   0          30s
janus-operator-7c8d9e6f4-xxxxx     1/1     Running   0          30s
janusd-xxxxx                        1/1     Running   0          30s
panoptes-eye-5f4e3d2c1-xxxxx       1/1     Running   0          30s
```

---

## 4. Create Test Application

```bash
# Deploy nginx test pod
kubectl apply -f - <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: test-app
  namespace: default
  labels:
    app: test-app
    panoptes.como-technologies.io/monitored: "true"
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    command: ["sleep", "infinity"]
EOF

# Wait for pod
kubectl wait --for=condition=Ready pod/test-app -n default --timeout=60s
```

---

## 5. Create ArgusWatcher (File Integrity Monitoring)

```bash
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
        - /etc/shadow
        - /etc/hosts
      events:
        - modify
        - create
        - delete
      recursive: false
      tags:
        severity: high
        compliance: pci-dss
EOF

# Verify watcher is active
kubectl get arguswatcher test-watcher -o wide
```

---

## 6. Create JanusGuard (Access Auditing)

```bash
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
  subjects:
    - deny:
        - /etc/shadow
      events:
        - access
        - open
      audit: true
      enforcing: false  # Dry-run mode for testing
      tags:
        severity: critical
EOF

# Verify guard is active
kubectl get janusguard test-guard -o wide
```

---

## 7. Access Panoptes Eye Dashboard

```bash
# Port-forward the UI
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000 &

# Open browser
open http://localhost:3000  # macOS
# or: xdg-open http://localhost:3000  # Linux
# or: start http://localhost:3000     # Windows
```

Dashboard features:
- **Watchers**: View all ArgusWatcher resources
- **Guards**: View all JanusGuard resources
- **Events**: Real-time file event stream
- **Explorer**: Browse container filesystems

---

## 8. Generate Test Events

```bash
# Trigger file modification (detected by Argus)
kubectl exec test-app -- sh -c "echo 'test' >> /etc/hosts"

# Trigger file access (audited by Janus)
kubectl exec test-app -- cat /etc/shadow

# View events in operator logs
kubectl logs -n panoptes-system -l app=argus-operator --tail=20
kubectl logs -n panoptes-system -l app=janus-operator --tail=20

# View events in Panoptes Eye
# Navigate to Events tab in the dashboard
```

---

## 9. Check Metrics (Optional)

```bash
# Argus operator metrics
kubectl port-forward -n panoptes-system svc/argus-operator-metrics 8080:8080 &
curl localhost:8080/metrics | grep argus_

# Janus operator metrics
kubectl port-forward -n panoptes-system svc/janus-operator-metrics 8081:8080 &
curl localhost:8081/metrics | grep janus_
```

---

## 10. Cleanup

```bash
# Delete test resources
kubectl delete arguswatcher test-watcher
kubectl delete janusguard test-guard
kubectl delete pod test-app

# Delete entire cluster
kind delete cluster --name panoptes-dev
```

---

## Troubleshooting

### Pods stuck in Pending
```bash
# Check events
kubectl describe pod -n panoptes-system <pod-name>

# Common issues:
# - Image not loaded into kind: kind load docker-image <image> --name panoptes-dev
# - Insufficient resources: Increase Docker memory to 4GB+
```

### Daemon pods failing
```bash
# Daemons need privileged access - check security context
kubectl get pod -n panoptes-system argusd-xxx -o yaml | grep -A5 securityContext

# Verify hostPID is enabled
kubectl get daemonset -n panoptes-system argusd -o yaml | grep hostPID
```

### No events detected
```bash
# Verify watcher is targeting correct pods
kubectl get arguswatcher test-watcher -o jsonpath='{.status.watchedPods}'

# Check daemon gRPC connectivity
kubectl logs -n panoptes-system argusd-xxx | grep -i grpc
```

### UI not loading
```bash
# Check UI pod logs
kubectl logs -n panoptes-system -l app=panoptes-eye

# Verify service account has permissions
kubectl auth can-i list pods --as=system:serviceaccount:panoptes-system:panoptes-eye
```

---

## Quick Reference

| Command | Description |
|---------|-------------|
| `./hack/local-deploy.sh build` | Build all images |
| `./hack/local-deploy.sh deploy` | Deploy full stack |
| `./hack/local-deploy.sh test` | Run test scenario |
| `./hack/local-deploy.sh clean` | Remove everything |
| `kubectl get aw` | List ArgusWatchers |
| `kubectl get jg` | List JanusGuards |

---

## Next Steps

1. **Enable webhook injection**: See [WEBHOOK_DEPLOYMENT.md](WEBHOOK_DEPLOYMENT.md) to ensure protection is active before apps start
2. **Add more watchers**: See `operators/argus-operator/config/samples/`
3. **Configure alerting**: Enable PrometheusRules in Helm values
4. **Deploy to Spectro Cloud**: Use `packs/panoptes/` for Palette deployment
5. **Read the docs**: `docs/FUTURE_STATE.md` for roadmap

---

## Optional: Enable Webhook Injection

By default, the local deployment runs without webhooks for simplicity. In production, you should enable webhooks to ensure file monitoring is active **before** your application containers start.

See [WEBHOOK_DEPLOYMENT.md](WEBHOOK_DEPLOYMENT.md) for complete instructions on:
- Installing cert-manager for TLS certificates
- Configuring the operators to enable webhooks
- Deploying MutatingWebhookConfigurations
- Enabling injection on namespaces

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
