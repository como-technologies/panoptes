# Troubleshooting Panoptes

This guide helps diagnose and resolve common issues with the Panoptes Suite (Argus file integrity monitoring and Janus access auditing).

## Quick Diagnostics

Start with these commands to get an overview of system health:

```bash
# Check all Panoptes components
kubectl get pods -n panoptes-system

# Check CRDs
kubectl get aw,jg -A

# Check operator logs
kubectl logs -n panoptes-system deploy/argus-operator -f
kubectl logs -n panoptes-system deploy/janus-operator -f

# Check daemon logs
kubectl logs -n panoptes-system ds/argusd -f
kubectl logs -n panoptes-system ds/janusd -f

# Check CRD status
kubectl describe aw <watcher-name> -n <namespace>
kubectl describe jg <guard-name> -n <namespace>
```

## Common Issues

### Daemon Won't Start

**Symptoms:**
- `argusd` or `janusd` pods stuck in `CrashLoopBackOff`
- Pods repeatedly restarting
- Error logs showing initialization failures

**Causes & Fixes:**

#### 1. Missing Capabilities
The daemons require specific Linux capabilities to access kernel interfaces.

**Check:**
```bash
kubectl get ds argusd -n panoptes-system -o yaml | grep -A 10 securityContext
```

**Fix:**
Ensure the DaemonSet `securityContext` includes:
```yaml
securityContext:
  capabilities:
    add:
    - SYS_ADMIN      # Required for fanotify
    - SYS_PTRACE     # Required to access container processes
    - DAC_READ_SEARCH # Required to bypass filesystem permissions
```

#### 2. Kernel Too Old
Panoptes requires Linux kernel 5.x or newer for proper inotify/fanotify support.

**Check:**
```bash
# From a node
uname -r

# Or from within a pod
kubectl exec -it <daemon-pod> -n panoptes-system -- uname -r
```

**Fix:**
Upgrade the node's kernel to 5.x or newer. If upgrading is not possible, use node selectors to avoid scheduling daemons on incompatible nodes.

#### 3. inotify/fanotify Not Available
Some minimal container-optimized OS images may have these features disabled.

**Check:**
```bash
# From a node
ls -la /proc/sys/fs/inotify/
ls -la /proc/sys/fs/fanotify/

# Check kernel config
grep -E "INOTIFY|FANOTIFY" /boot/config-$(uname -r)
```

**Fix:**
Ensure kernel is compiled with `CONFIG_INOTIFY_USER=y` and `CONFIG_FANOTIFY=y`. This typically requires using a different base OS image or recompiling the kernel.

#### 4. Container Runtime Socket Not Found
Daemons auto-detect the container runtime by probing socket paths.

**Check:**
```bash
kubectl logs <daemon-pod> -n panoptes-system | grep "container runtime"
```

**Fix:**
If auto-detection fails, explicitly specify the runtime in your CRD:
```yaml
spec:
  containerRuntime: containerd  # or crio
```

Or check that the DaemonSet has proper socket mounts:
```yaml
volumeMounts:
- name: containerd-sock
  mountPath: /var/run/containerd/containerd.sock
volumes:
- name: containerd-sock
  hostPath:
    path: /var/run/containerd/containerd.sock
```

### No Events Appearing

**Symptoms:**
- ArgusWatcher status shows `observablePods > 0` and `watchedPods > 0`
- But `eventsDetected` remains at 0
- No events in operator logs

**Causes & Fixes:**

#### 1. Wrong Pod Labels
The selector may not match any pods, or matches different pods than expected.

**Check:**
```bash
# See what pods your selector matches
kubectl get pods -l <your-selector> -n <namespace>

# Example
kubectl get pods -l app=nginx -n default
```

**Fix:**
Update the `spec.selector` in your ArgusWatcher to match the correct pods:
```yaml
spec:
  selector:
    matchLabels:
      app: my-app
```

#### 2. Paths Don't Exist in Container
The monitored paths may not exist in the target container's filesystem.

**Check:**
```bash
# Exec into the target pod and verify paths
kubectl exec <pod> -n <namespace> -- ls -la /etc/config
```

**Fix:**
- Update `spec.paths` to use paths that actually exist in the container
- Remember that container filesystems are isolated and may differ from the host

#### 3. No File Activity on Monitored Paths
Events only appear when files are actually modified.

**Test:**
```bash
# Trigger a test event
kubectl exec <pod> -n <namespace> -- touch /etc/test-file
kubectl exec <pod> -n <namespace> -- rm /etc/test-file
```

**Fix:**
If test events appear, your monitoring is working correctly. Wait for actual file changes or verify you're monitoring the right paths.

#### 4. Wrong Event Types
You may be watching for events that aren't occurring.

**Check:**
```yaml
spec:
  eventTypes:
  - modify    # Only fires on content changes, not creation
  - create    # Only fires on new files
  - delete    # Only fires on deletions
```

**Fix:**
Use `all` to capture all event types, or specify the correct types for your use case:
```yaml
spec:
  eventTypes:
  - all  # Captures create, modify, delete, move, attrib
```

#### 5. Ignore Patterns Too Broad
Your ignore list may be filtering out all events.

**Check:**
```yaml
spec:
  ignore:
  - "/etc/*"  # This ignores everything under /etc
```

**Fix:**
Make ignore patterns more specific:
```yaml
spec:
  ignore:
  - "/etc/*.log"      # Only ignore log files
  - "/etc/cache/**/*" # Only ignore cache directory
```

### Queue Overflow Alerts

**Symptoms:**
- Prometheus alert `PanoptesInotifyQueueOverflow` firing
- Daemon metrics show `queue_overflows > 0`
- Events being dropped
- Warning logs: "inotify queue overflow"

**Causes & Fixes:**

#### 1. Kernel Queue Too Small
The default `fs.inotify.max_queued_events` (16384 on many systems) may be insufficient for high-traffic directories.

**Check:**
```bash
# From a node
sysctl fs.inotify.max_queued_events
cat /proc/sys/fs/inotify/max_queued_events

# From within a privileged pod
kubectl exec <daemon-pod> -n panoptes-system -- cat /proc/sys/fs/inotify/max_queued_events
```

**Fix:**
Increase the kernel parameter (requires node-level access):
```bash
# Temporary (until reboot)
sudo sysctl -w fs.inotify.max_queued_events=65536

# Permanent
echo "fs.inotify.max_queued_events=65536" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

For Kubernetes nodes, use a DaemonSet with init container:
```yaml
initContainers:
- name: sysctl
  image: busybox
  command:
  - sh
  - -c
  - sysctl -w fs.inotify.max_queued_events=65536
  securityContext:
    privileged: true
```

#### 2. Too Many Watched Paths
Watching deep directory trees generates many events.

**Check:**
```bash
# Count watches
kubectl exec <daemon-pod> -n panoptes-system -- cat /proc/<argusd-pid>/fdinfo/* | grep inotify | wc -l
```

**Fix:**
Reduce scope by:
- Limiting `maxDepth` for recursive watches
- Watching fewer paths
- Using more specific paths instead of broad directory trees

```yaml
spec:
  paths:
  - path: /etc
    maxDepth: 2  # Only watch 2 levels deep
```

#### 3. High-Throughput Directory
Some directories (logs, cache, temp) generate many events.

**Check:**
Look for patterns in event logs showing specific directories with high activity.

**Fix:**
- Add high-throughput directories to ignore list
- Use specific event types instead of "all"
- Consider if you really need to monitor these directories

```yaml
spec:
  ignore:
  - "/var/log/**/*"
  - "/tmp/**/*"
  - "/var/cache/**/*"
```

### ConfigMap/Secret Updates Not Detected

**Symptoms:**
- ConfigMap or Secret mounted as volume
- Updates to ConfigMap/Secret in Kubernetes
- No events generated when content changes

**Cause:**
Kubernetes uses atomic symlink swaps for ConfigMap/Secret mounts. The typical pattern is:
1. New data written to `..data_tmp`
2. `..data` symlink updated to point to new directory
3. Individual file symlinks point to `..data/<filename>`

inotify watches on the final symlink target don't fire when Kubernetes updates the intermediate `..data` symlink.

**Fix:**
Watch the parent directory instead of individual files:

**Wrong:**
```yaml
spec:
  paths:
  - path: /etc/config/app.conf  # Individual file - won't detect updates
```

**Correct:**
```yaml
spec:
  paths:
  - path: /etc/config  # Parent directory
  eventTypes:
  - movedto   # Fires when ..data symlink is updated
  - movedfrom # Fires on old symlink removal
  - create    # Fires for new files
```

**Verify:**
```bash
# Watch the mount structure
kubectl exec <pod> -- ls -la /etc/config
# You'll see ..data, ..data_tmp, and individual file symlinks
```

### OverlayFS Monitoring Behavior

**Symptoms:**
- Events appear for file writes (modify, create, delete)
- No events for file reads
- Missing events for base image files

**Cause:**
inotify monitors the OverlayFS upper (writable) layer. Container base image files exist in the lower (read-only) layer and don't generate inotify events. Read operations on lower-layer files don't modify the filesystem, so no events fire.

**Explanation:**
This is expected behavior for inotify. It's a modification-tracking interface, not an access-tracking interface.

**Fix:**
- For integrity monitoring (detecting unauthorized changes), this is correct behavior - you want to know when files are modified
- For access auditing (detecting who reads files), use JanusGuard with fanotify instead:

```yaml
apiVersion: janus.panoptes-suite.io/v2
kind: JanusGuard
metadata:
  name: audit-sensitive-reads
spec:
  selector:
    matchLabels:
      app: my-app
  paths:
  - path: /etc/secrets
  operations:
  - open_read
  - open_exec
  auditMode: true
```

### Container Runtime Detection Failures

**Symptoms:**
- Daemon logs show "failed to detect container runtime"
- Daemon crashes or fails to monitor containers
- Error: "no container runtime socket found"

**Causes & Fixes:**

#### 1. Non-Standard Socket Path
Some Kubernetes distributions use custom socket paths.

**Check:**
```bash
# Common socket paths
ls -la /var/run/containerd/containerd.sock
ls -la /var/run/crio/crio.sock
ls -la /run/containerd/containerd.sock
```

**Fix:**
Explicitly set the runtime in your CRD:
```yaml
spec:
  containerRuntime: containerd  # or crio
  containerRuntimeSocket: /custom/path/to/containerd.sock
```

#### 2. Socket Not Mounted
The DaemonSet may not have the socket mounted.

**Check:**
```bash
kubectl exec <daemon-pod> -n panoptes-system -- ls -la /var/run/containerd/
```

**Fix:**
Update the DaemonSet to mount the socket:
```yaml
spec:
  template:
    spec:
      volumes:
      - name: containerd-sock
        hostPath:
          path: /var/run/containerd/containerd.sock
          type: Socket
      containers:
      - name: argusd
        volumeMounts:
        - name: containerd-sock
          mountPath: /var/run/containerd/containerd.sock
```

#### 3. Permission Denied on Socket
The daemon may lack permissions to access the socket.

**Check:**
```bash
kubectl logs <daemon-pod> -n panoptes-system | grep "permission denied"
```

**Fix:**
Ensure the daemon runs with appropriate capabilities and/or as a privileged container:
```yaml
securityContext:
  privileged: true  # Or use specific capabilities
```

### gRPC Connection Issues

**Symptoms:**
- Operator logs show "failed to connect to daemon"
- Errors: "connection refused", "deadline exceeded"
- Status shows `connectedDaemons < observablePods`

**Causes & Fixes:**

#### 1. Daemon Not Running on That Node
The operator tries to connect to a daemon that isn't scheduled or running.

**Check:**
```bash
# See which nodes have daemon pods
kubectl get pods -n panoptes-system -o wide | grep argusd

# Check if DaemonSet has node selectors or taints preventing scheduling
kubectl describe ds argusd -n panoptes-system
```

**Fix:**
- Ensure DaemonSet node selectors match your nodes
- Check node taints and add tolerations if needed
- Verify the node is ready and schedulable

#### 2. NetworkPolicy Blocking Traffic
NetworkPolicies may block operator-to-daemon communication.

**Check:**
```bash
kubectl get networkpolicies -n panoptes-system
kubectl describe networkpolicy <policy-name> -n panoptes-system
```

**Fix:**
Ensure NetworkPolicies allow:
- Operator to daemon on ports 50051 (argusd) and 50052 (janusd)
- Protocol: TCP

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-operator-to-daemons
spec:
  podSelector:
    matchLabels:
      app: argusd
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: argus-operator
    ports:
    - protocol: TCP
      port: 50051
```

#### 3. DNS Resolution Failure
The operator can't resolve daemon service names.

**Check:**
```bash
# Test DNS from operator pod
kubectl exec <operator-pod> -n panoptes-system -- nslookup argusd.panoptes-system.svc.cluster.local

# Check CoreDNS/kube-dns
kubectl get pods -n kube-system | grep -E "coredns|kube-dns"
```

**Fix:**
- Ensure CoreDNS or kube-dns is healthy
- Restart operator pod to refresh DNS cache
- Check for cluster DNS configuration issues

### Watch Limit Exhaustion

**Symptoms:**
- New watches fail to be created
- Error logs: "inotify_add_watch: No space left on device"
- errno 28 (ENOSPC)
- Daemon metrics show watches approaching limit

**Cause:**
The kernel parameter `fs.inotify.max_user_watches` limits total inotify watches per user. This is shared across all containers on a node. Default is often 8192 or 65536.

**Check:**
```bash
# Check current limit
sysctl fs.inotify.max_user_watches
cat /proc/sys/fs/inotify/max_user_watches

# Check current usage (rough estimate)
find /proc/*/fd -lname 'anon_inode:inotify' 2>/dev/null | wc -l
```

**Fix:**

Increase the kernel limit:
```bash
# Temporary
sudo sysctl -w fs.inotify.max_user_watches=524288

# Permanent
echo "fs.inotify.max_user_watches=524288" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

Reduce watch count:
```yaml
spec:
  paths:
  - path: /etc
    maxDepth: 1  # Limit recursion depth
  ignore:
  - "/etc/ssl/**/*"  # Exclude large directories
  - "/etc/locale/**/*"
```

**Related Parameters:**
```bash
# Maximum watches per user (default: 8192-65536)
fs.inotify.max_user_watches=524288

# Maximum queued events (default: 16384)
fs.inotify.max_queued_events=65536

# Maximum inotify instances per user (default: 128)
fs.inotify.max_user_instances=256
```

### JanusGuard Not Blocking Access

**Symptoms:**
- Deny rules configured in JanusGuard
- Access attempts still succeed
- No permission denied errors

**Causes & Fixes:**

#### 1. Enforcing Mode Disabled
JanusGuard may be in audit-only mode.

**Check:**
```bash
kubectl get jg <guard-name> -n <namespace> -o yaml | grep enforcing
```

**Fix:**
Enable enforcing mode:
```yaml
spec:
  enforcing: true  # Must be true to actually block access
```

#### 2. Auto-Allow Owner
By default, file owners can always access their own files.

**Check:**
```yaml
spec:
  autoAllowOwner: true  # Default
```

**Fix:**
If you need to deny even owner access:
```yaml
spec:
  autoAllowOwner: false
```

#### 3. Allow Rule Overriding Deny
Rule evaluation order matters: deny > allow > defaultResponse.

**Check:**
Review your rules for conflicts:
```yaml
spec:
  rules:
  - pattern: /etc/passwd
    principals:
    - uid: 1000
    operations:
    - open_read
    decision: allow  # This takes precedence over deny rules
```

**Fix:**
Ensure deny rules are properly scoped and not overridden by broader allow rules. More specific rules should come first.

#### 4. Paused Mode
The guard may be paused.

**Check:**
```bash
kubectl get jg <guard-name> -n <namespace> -o yaml | grep paused
```

**Fix:**
Unpause the guard:
```yaml
spec:
  paused: false
```

### Dashboard Not Loading

**Symptoms:**
- Browser shows 502 Bad Gateway
- Blank page
- Connection refused
- Dashboard pod exists but unreachable

**Causes & Fixes:**

#### 1. Port-Forward Not Active
The local port-forward may have disconnected.

**Check:**
```bash
# Check if port-forward process is running
ps aux | grep port-forward

# Test local connection
curl http://localhost:3000
```

**Fix:**
Restart port-forward:
```bash
kubectl port-forward -n panoptes-system svc/panoptes-eye 3000:3000
```

For persistent access, consider using an Ingress instead.

#### 2. API Server Unreachable from Dashboard Pod
The dashboard may not be able to reach the Kubernetes API server.

**Check:**
```bash
kubectl logs <dashboard-pod> -n panoptes-system

# Test API access from pod
kubectl exec <dashboard-pod> -n panoptes-system -- curl -k https://kubernetes.default.svc
```

**Fix:**
- Ensure the pod has network connectivity to the API server
- Check NetworkPolicies
- Verify ServiceAccount token is mounted

#### 3. RBAC Permissions
The dashboard ServiceAccount may lack permissions.

**Check:**
```bash
kubectl describe sa panoptes-eye -n panoptes-system
kubectl get rolebindings,clusterrolebindings -A | grep panoptes-eye
```

**Fix:**
Ensure the dashboard has read access to ArgusWatcher and JanusGuard resources:
```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: panoptes-eye-reader
rules:
- apiGroups: ["argus.panoptes-suite.io"]
  resources: ["arguswatchers"]
  verbs: ["get", "list", "watch"]
- apiGroups: ["janus.panoptes-suite.io"]
  resources: ["janusguards"]
  verbs: ["get", "list", "watch"]
```

## Diagnostic Commands Reference

### Component Health
```bash
# All components
kubectl get all -n panoptes-system

# Just pods with node placement
kubectl get pods -n panoptes-system -o wide

# Daemon status on each node
kubectl get pods -n panoptes-system -l app=argusd -o wide

# Operator status
kubectl get deploy -n panoptes-system
```

### CRD Inspection
```bash
# List all watchers and guards
kubectl get aw,jg -A

# Full details
kubectl describe aw <name> -n <namespace>
kubectl get aw <name> -n <namespace> -o yaml

# Status only
kubectl get aw <name> -n <namespace> -o jsonpath='{.status}'
```

### Logs
```bash
# Follow operator logs
kubectl logs -n panoptes-system deploy/argus-operator -f
kubectl logs -n panoptes-system deploy/janus-operator -f

# Follow daemon logs (all pods)
kubectl logs -n panoptes-system -l app=argusd -f --all-containers=true

# Specific daemon pod
kubectl logs -n panoptes-system <pod-name> -f

# Previous logs (if crashed)
kubectl logs -n panoptes-system <pod-name> --previous
```

### Metrics
```bash
# Port-forward to operator metrics
kubectl port-forward -n panoptes-system deploy/argus-operator 8080:8080
curl http://localhost:8080/metrics

# From within daemon pod
kubectl exec <daemon-pod> -n panoptes-system -- curl http://localhost:50051/metrics

# From within operator pod
kubectl exec <operator-pod> -n panoptes-system -- curl http://localhost:8080/metrics
```

### Events
```bash
# Kubernetes events for namespace
kubectl get events -n panoptes-system --sort-by='.lastTimestamp'

# Events for specific resource
kubectl describe aw <name> -n <namespace> | grep -A 20 Events

# Watch events in real-time
kubectl get events -n panoptes-system --watch
```

### Network Debugging
```bash
# Test gRPC connectivity from operator to daemon
kubectl exec <operator-pod> -n panoptes-system -- nc -zv argusd.panoptes-system.svc.cluster.local 50051

# DNS resolution
kubectl exec <operator-pod> -n panoptes-system -- nslookup argusd.panoptes-system.svc.cluster.local

# Check NetworkPolicies
kubectl get networkpolicies -n panoptes-system
```

### Kernel Parameters
```bash
# Check inotify limits (from node or privileged pod)
sysctl fs.inotify.max_user_watches
sysctl fs.inotify.max_queued_events
sysctl fs.inotify.max_user_instances

# Or read directly
cat /proc/sys/fs/inotify/max_user_watches
cat /proc/sys/fs/inotify/max_queued_events
cat /proc/sys/fs/inotify/max_user_instances
```

### Container Runtime
```bash
# Check runtime socket
kubectl exec <daemon-pod> -n panoptes-system -- ls -la /var/run/containerd/containerd.sock
kubectl exec <daemon-pod> -n panoptes-system -- ls -la /var/run/crio/crio.sock

# Test runtime access (if crictl is available)
kubectl exec <daemon-pod> -n panoptes-system -- crictl ps
```

### Resource Usage
```bash
# CPU and memory usage
kubectl top pods -n panoptes-system

# Resource requests/limits
kubectl describe pod <pod-name> -n panoptes-system | grep -A 5 "Limits:"
```

## Getting Help

If you're still experiencing issues after trying these troubleshooting steps:

1. **File an issue**: Report bugs or request help at the GitHub repository with:
   - Kubernetes version (`kubectl version`)
   - Kernel version (`uname -r`)
   - Container runtime and version
   - Relevant logs and CRD definitions
   - Steps to reproduce

2. **Check daemon metrics**:
   ```bash
   kubectl exec <daemon-pod> -n panoptes-system -- curl http://localhost:50051/metrics
   ```
   Look for:
   - `panoptes_queue_overflows` - Event queue overflow count
   - `panoptes_watches_active` - Current active watch count
   - `panoptes_events_detected_total` - Total events detected

3. **Check operator metrics**:
   ```bash
   kubectl exec <operator-pod> -n panoptes-system -- curl http://localhost:8080/metrics
   ```
   Look for:
   - `controller_runtime_reconcile_total` - Reconciliation attempts
   - `controller_runtime_reconcile_errors_total` - Reconciliation errors
   - `workqueue_depth` - Work queue backlog

4. **Enable debug logging**:
   Edit the operator Deployment to increase log verbosity:
   ```bash
   kubectl set env deploy/argus-operator -n panoptes-system LOG_LEVEL=debug
   ```

5. **Collect support bundle**:
   ```bash
   # Dump all relevant information
   kubectl get all,aw,jg -A -o yaml > panoptes-state.yaml
   kubectl logs -n panoptes-system -l app=argusd --tail=1000 > argusd-logs.txt
   kubectl logs -n panoptes-system deploy/argus-operator --tail=1000 > operator-logs.txt
   kubectl describe nodes > nodes.txt
   ```

Remember to sanitize any sensitive information (secrets, tokens, internal hostnames) before sharing logs publicly.
