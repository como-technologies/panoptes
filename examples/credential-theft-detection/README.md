# Credential Theft Detection

Detect and audit attempts to access credentials inside containers using both ArgusWatcher (detect modifications) and JanusGuard (audit/block access).

## What This Demonstrates

- **ArgusWatcher**: Detects modifications to credential files (new SSH keys, password changes, token tampering)
- **JanusGuard**: Audits and optionally blocks read access to credential files
- **Defense in depth**: Using both Argus and Janus together on the same paths for comprehensive coverage

## What Credential Theft Looks Like in Containers

Attackers who gain code execution inside a container immediately look for credentials they can use to escalate privileges or move laterally:

| Target | What Attackers Want | Real-World Impact |
|--------|--------------------|--------------------|
| `/etc/shadow` | Password hashes for offline cracking | Lateral movement to other systems sharing credentials |
| `/etc/passwd` | User enumeration, UID 0 account injection | Privilege escalation within the container |
| `/root/.ssh/*` | SSH private keys, authorized_keys | Lateral movement to other hosts |
| `/var/run/secrets/kubernetes.io` | Service account tokens | Kubernetes API access, cluster compromise |
| `/home/*/.ssh/*` | User SSH keys | Lateral movement, identity impersonation |
| `/root/.aws/credentials` | Cloud provider credentials | Cloud account takeover |
| `/root/.kube/config` | Kubernetes configs | Cluster admin access |

Panoptes watches for both the **access** (reading) and **modification** (tampering) of these files.

## Automated Demo

```bash
./demo.sh

# Clean up
./demo.sh --cleanup
```

## Step-by-Step Walkthrough

### Step 1: Deploy the Workload with Monitoring

```bash
kubectl apply -f attacker-simulation.yaml
kubectl wait --for=condition=available deploy/target-workload --timeout=120s
```

This creates:
- A deployment labeled `panoptes.como-technologies.io/monitored: "true"`
- An ArgusWatcher detecting credential file modifications
- A JanusGuard auditing credential file access attempts

### Step 2: Verify Both ArgusWatcher and JanusGuard

```bash
kubectl get aw credential-fim -o wide
kubectl get jg credential-access -o wide
```

Expected output:
```
NAME             AGE   STATUS
credential-fim   30s   Active

NAME                AGE   STATUS
credential-access   30s   Active
```

### Step 3: Simulate Credential Theft Attempts

```bash
POD=$(kubectl get pods -l app=target-workload -o jsonpath='{.items[0].metadata.name}')

# Read /etc/shadow (password hash theft)
kubectl exec "$POD" -- cat /etc/shadow

# Read Kubernetes service account token
kubectl exec "$POD" -- cat /var/run/secrets/kubernetes.io/serviceaccount/token

# Inject SSH authorized key
kubectl exec "$POD" -- bash -c "mkdir -p /root/.ssh && echo 'ssh-rsa AAAA...' > /root/.ssh/authorized_keys"

# Modify /etc/passwd (add backdoor user)
kubectl exec "$POD" -- bash -c "echo 'backdoor:x:0:0::/root:/bin/bash' >> /etc/passwd"
```

### Step 4: View Detections

```bash
# ArgusWatcher events (file modifications)
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argusd --tail=20

# JanusGuard events (file access)
kubectl logs -n panoptes-system -l app.kubernetes.io/name=janusd --tail=20
```

Expected events (abbreviated):
```
# ArgusWatcher (FIM) events:
{"event":"create","path":"/root/.ssh/authorized_keys","tags":{"category":"ssh-keys","severity":"critical"}}
{"event":"modify","path":"/etc/passwd","tags":{"category":"credential-modification","severity":"critical"}}

# JanusGuard (access audit) events:
{"event":"open","path":"/etc/shadow","tags":{"category":"credential-access","severity":"critical"},"action":"denied"}
{"event":"open","path":"/var/run/secrets/kubernetes.io/serviceaccount/token","tags":{"category":"k8s-secrets","severity":"critical"}}
```

## Files

| File | Description |
|------|-------------|
| `attacker-simulation.yaml` | Deployment + ArgusWatcher + JanusGuard (all-in-one) |
| `demo.sh` | Automated demo script with cleanup support |

## Defense in Depth: Why Both Argus and Janus?

| Attack | Argus Detects | Janus Detects |
|--------|:------------:|:-------------:|
| Read /etc/shadow | | Yes (access event) |
| Modify /etc/passwd | Yes (modify event) | |
| Create SSH key | Yes (create event) | |
| Read SSH private key | | Yes (open event) |
| Read K8s service account token | | Yes (open event) |
| Modify /etc/shadow | Yes (modify event) | Yes (open event) |

**Argus** tells you when credential files are modified (write operations).
**Janus** tells you when credential files are read (read operations).

Together, they provide complete visibility into credential file activity.

## Enabling Enforcement Mode

After validating in audit mode (no false positives from legitimate processes), enable blocking:

```bash
kubectl patch jg credential-access -p '{"spec":{"enforcing":true}}'
```

With enforcement enabled, JanusGuard will deny access to files in the `deny` list, preventing credential theft even after the attacker has code execution.
