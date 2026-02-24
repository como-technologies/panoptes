# Detect Container Breakout Indicators

Detect the telltale signs of a container breakout attempt using Panoptes ArgusWatcher with MITRE ATT&CK mappings.

## What This Detects

Container breakouts follow a predictable pattern. Attackers who gain code execution inside a container typically perform these steps before attempting to escape to the host:

| Indicator | MITRE ATT&CK | Why It Matters |
|-----------|---------------|----------------|
| Files created in /tmp or /dev/shm | [T1074 - Data Staged](https://attack.mitre.org/techniques/T1074/) | Attackers stage tools and payloads in world-writable directories before executing them |
| Cron jobs added | [T1053.003 - Scheduled Task: Cron](https://attack.mitre.org/techniques/T1053/003/) | Persistence mechanism -- survives container restarts if writable |
| SSH keys injected | [T1098.004 - Account Manipulation: SSH Authorized Keys](https://attack.mitre.org/techniques/T1098/004/) | Provides persistent backdoor access to the container or host |
| /etc/ld.so.preload modified | [T1574.006 - Hijack Execution Flow: LD_PRELOAD](https://attack.mitre.org/techniques/T1574/006/) | Library injection -- every process loads the attacker's code |
| /etc/passwd modified | [T1136.001 - Create Account: Local Account](https://attack.mitre.org/techniques/T1136/001/) | Adding a backdoor user with UID 0 (root equivalent) |

## How It Works

The `arguswatcher.yaml` creates an ArgusWatcher that monitors all five indicator paths. Each subject is tagged with the corresponding MITRE ATT&CK technique ID, making events immediately actionable for incident response.

## Automated Demo

```bash
./demo.sh

# Clean up
./demo.sh --cleanup
```

## Step-by-Step Walkthrough

### Step 1: Deploy a Vulnerable Workload

The vulnerable-pod simulates an application with a remote code execution vulnerability:

```bash
kubectl apply -f vulnerable-pod.yaml
kubectl wait --for=condition=available deploy/vulnerable-app --timeout=120s
```

### Step 2: Apply the Breakout Detection ArgusWatcher

```bash
kubectl apply -f arguswatcher.yaml
```

This creates an ArgusWatcher that monitors:
- `/tmp` and `/dev/shm` for tool staging
- `/etc/crontab` and `/etc/cron.d` for cron persistence
- `/root/.ssh` for SSH key injection
- `/etc/ld.so.preload` for library hijacking
- `/etc/passwd` for backdoor user creation

### Step 3: Simulate a Container Breakout Attempt

```bash
POD=$(kubectl get pods -l app=vulnerable-app -o jsonpath='{.items[0].metadata.name}')

# Stage tools in /tmp (T1074)
kubectl exec "$POD" -- bash -c "echo '#!/bin/bash' > /tmp/linpeas.sh && chmod +x /tmp/linpeas.sh"

# Stage payload in /dev/shm (T1074)
kubectl exec "$POD" -- bash -c "echo 'exploit payload' > /dev/shm/payload.bin"

# Add a cron job for persistence (T1053.003)
kubectl exec "$POD" -- bash -c "echo '* * * * * root /tmp/linpeas.sh' >> /etc/crontab"

# Inject an SSH key (T1098.004)
kubectl exec "$POD" -- bash -c "mkdir -p /root/.ssh && echo 'ssh-rsa AAAA... attacker@evil.com' > /root/.ssh/authorized_keys"

# Modify ld.so.preload for library injection (T1574.006)
kubectl exec "$POD" -- bash -c "echo '/tmp/libevil.so' > /etc/ld.so.preload"

# Add a backdoor user (T1136.001)
kubectl exec "$POD" -- bash -c "echo 'backdoor:x:0:0::/root:/bin/bash' >> /etc/passwd"
```

### Step 4: Observe Detections

```bash
# Check ArgusWatcher status
kubectl get aw breakout-detection -o wide

# View detection events in operator logs
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argus-operator --tail=30

# View kernel-level events from the daemon
kubectl logs -n panoptes-system -l app.kubernetes.io/name=argusd --tail=30
```

Expected log events (abbreviated):
```
{"event":"create","path":"/tmp/linpeas.sh","tags":{"attack":"T1074","category":"staging"},"severity":"high"}
{"event":"create","path":"/dev/shm/payload.bin","tags":{"attack":"T1074","category":"staging"},"severity":"high"}
{"event":"modify","path":"/etc/crontab","tags":{"attack":"T1053.003","category":"cron-persistence"},"severity":"critical"}
{"event":"create","path":"/root/.ssh/authorized_keys","tags":{"attack":"T1098.004","category":"ssh-keys"},"severity":"critical"}
{"event":"modify","path":"/etc/ld.so.preload","tags":{"attack":"T1574.006","category":"library-injection"},"severity":"critical"}
{"event":"modify","path":"/etc/passwd","tags":{"attack":"T1136.001","category":"account-creation"},"severity":"critical"}
```

## Files

| File | Description |
|------|-------------|
| `vulnerable-pod.yaml` | Ubuntu-based deployment simulating a vulnerable application |
| `arguswatcher.yaml` | ArgusWatcher with MITRE ATT&CK-tagged breakout indicators |
| `demo.sh` | Automated demo script with cleanup support |

## Incident Response Context

When you see these events in production, here is what to do:

1. **T1074 (Staging)**: Isolate the pod immediately. Examine what was staged and determine the initial access vector.
2. **T1053.003 (Cron)**: The attacker has persistence. Check if the cron job has already executed. Review container image for known vulnerabilities.
3. **T1098.004 (SSH Keys)**: The attacker may already have persistent access. Rotate all credentials. Check if the container has SSH exposed.
4. **T1574.006 (LD_PRELOAD)**: This is a sophisticated attack. All processes in the container are potentially compromised. Terminate and rebuild.
5. **T1136.001 (Backdoor User)**: The attacker has root-equivalent access. The container is fully compromised.
