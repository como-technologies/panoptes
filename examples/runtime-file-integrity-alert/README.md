# Runtime File Integrity Alert

The simplest Panoptes demo. Deploy nginx, monitor its config and content directories, then watch Panoptes detect changes in real-time.

## What This Demonstrates

- Basic ArgusWatcher setup for file integrity monitoring
- Monitoring nginx configuration files (`/etc/nginx`) for tampering
- Monitoring web content (`/usr/share/nginx/html`) for defacement
- Real-time detection via Linux inotify at the kernel level

## Why This Matters

In production, nginx configuration should never change at runtime. If it does, it means either:

1. **Misconfiguration**: Someone is `kubectl exec`-ing into pods to make changes (bad practice)
2. **Compromise**: An attacker has gained code execution and is modifying the web server
3. **Supply chain attack**: The container image was tampered with during build

Panoptes detects all three scenarios the same way -- by watching for file modifications at the kernel level.

## Automated Demo

```bash
./demo.sh

# Clean up
./demo.sh --cleanup
```

## Step-by-Step Walkthrough

### Step 1: Deploy Nginx with Monitoring

```bash
kubectl apply -f nginx-monitored.yaml
kubectl wait --for=condition=available deploy/nginx-monitored --timeout=120s
```

This creates:
- An nginx Deployment with the `panoptes.como-technologies.io/monitored: "true"` label
- An ArgusWatcher that monitors `/etc/nginx` and `/usr/share/nginx/html`

### Step 2: Verify Monitoring is Active

```bash
kubectl get aw nginx-fim -o wide
```

Expected output:
```
NAME        AGE   STATUS
nginx-fim   30s   Active
```

### Step 3: Simulate Config Tampering

```bash
POD=$(kubectl get pods -l app=nginx-monitored -o jsonpath='{.items[0].metadata.name}')

# Modify nginx.conf (simulates config tampering)
kubectl exec "$POD" -- sh -c "echo '# backdoor config' >> /etc/nginx/nginx.conf"
```

### Step 4: Simulate Web Content Defacement

```bash
# Deface the default page
kubectl exec "$POD" -- sh -c "echo '<h1>HACKED</h1>' > /usr/share/nginx/html/index.html"
```

### Step 5: View the Alerts

```bash
# View ArgusWatcher status
kubectl get aw nginx-fim -o wide

# View detection events
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argusd --tail=20
```

Expected log events:
```
{"event":"modify","path":"/etc/nginx/nginx.conf","tags":{"severity":"critical","category":"config"},"pod":"nginx-monitored-..."}
{"event":"modify","path":"/usr/share/nginx/html/index.html","tags":{"severity":"high","category":"content"},"pod":"nginx-monitored-..."}
```

## Files

| File | Description |
|------|-------------|
| `nginx-monitored.yaml` | Nginx Deployment + ArgusWatcher (single file for simplicity) |
| `demo.sh` | Automated demo script with cleanup support |

## What to Try Next

- Add more paths to monitor (e.g., `/etc/ssl/certs` for certificate changes)
- Add a JanusGuard to block access to `/etc/nginx/nginx.conf` entirely
- Set up a Prometheus alert on ArgusWatcher events for the `severity: critical` tag
