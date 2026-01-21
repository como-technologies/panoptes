# Kernel Tuning for Panoptes

This guide covers Linux kernel parameter tuning for optimal Panoptes performance and security.

## Overview

Panoptes daemons use Linux kernel interfaces:
- **argusd**: Uses inotify for file integrity monitoring
- **janusd**: Uses fanotify for file access control and auditing

Both have kernel-imposed limits that may need tuning in high-activity environments.

## inotify Tuning (argusd)

### Key Parameters

| Parameter | Default | Location | Purpose |
|-----------|---------|----------|---------|
| `max_queued_events` | 16,384 | `/proc/sys/fs/inotify/max_queued_events` | Max events queued before overflow |
| `max_user_instances` | 128 | `/proc/sys/fs/inotify/max_user_instances` | Max inotify instances per user |
| `max_user_watches` | 8,192 | `/proc/sys/fs/inotify/max_user_watches` | Max watches per user |

### Security Risk: Queue Overflow Attack

An attacker can intentionally overflow the inotify queue to cause event loss:

```bash
# Attack pattern - generates events faster than queue can handle
for i in $(seq 1 20000); do
  touch /tmp/flood_$i && rm /tmp/flood_$i
done &
# During overflow, modifications to monitored files are LOST
```

When queue overflows:
1. Kernel generates `IN_Q_OVERFLOW` event
2. All events during overflow window are **permanently lost**
3. Daemon may not immediately detect which files were modified

### Recommended Settings

```bash
# Production systems with moderate activity
echo 65536 > /proc/sys/fs/inotify/max_queued_events
echo 256 > /proc/sys/fs/inotify/max_user_instances
echo 524288 > /proc/sys/fs/inotify/max_user_watches

# High-activity systems (CI/CD, build servers)
echo 131072 > /proc/sys/fs/inotify/max_queued_events
echo 512 > /proc/sys/fs/inotify/max_user_instances
echo 1048576 > /proc/sys/fs/inotify/max_user_watches
```

### Persistent Configuration

Create `/etc/sysctl.d/99-panoptes-inotify.conf`:

```ini
# Panoptes inotify tuning
# Increase queue size to prevent overflow attacks
fs.inotify.max_queued_events = 65536

# Increase watch limit for broad monitoring
fs.inotify.max_user_watches = 524288

# Increase instance limit for multiple watchers
fs.inotify.max_user_instances = 256
```

Apply immediately:

```bash
sysctl --system
```

### Kubernetes DaemonSet Configuration

For Kubernetes deployments, set sysctls via init container or privileged container:

```yaml
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: argusd
spec:
  template:
    spec:
      initContainers:
        - name: sysctl-tuning
          image: busybox
          securityContext:
            privileged: true
          command:
            - sh
            - -c
            - |
              sysctl -w fs.inotify.max_queued_events=65536
              sysctl -w fs.inotify.max_user_watches=524288
              sysctl -w fs.inotify.max_user_instances=256
```

Or via Kubernetes sysctl support (requires kubelet configuration):

```yaml
spec:
  template:
    spec:
      securityContext:
        sysctls:
          - name: fs.inotify.max_queued_events
            value: "65536"
          - name: fs.inotify.max_user_watches
            value: "524288"
```

**Note**: `fs.inotify.*` sysctls are namespaced and may require `--allowed-unsafe-sysctls` in kubelet configuration.

### Monitoring Queue Health

Check current inotify usage:

```bash
# Count watches per process
for pid in $(ls /proc | grep -E '^[0-9]+$'); do
  [ -d /proc/$pid/fd ] && \
    watches=$(ls -l /proc/$pid/fd 2>/dev/null | grep inotify | wc -l)
  [ $watches -gt 0 ] && echo "$pid: $watches watches"
done

# Check queue depth (requires kernel debug)
cat /sys/kernel/debug/inotify/total_watches 2>/dev/null || echo "Debug not available"
```

Prometheus metrics to monitor:

```promql
# Watch count approaching limit
panoptes_argus_inotify_watches / on() group_left()
  panoptes_argus_inotify_max_watches > 0.8

# Queue overflow events
increase(panoptes_argus_inotify_queue_overflow_total[5m]) > 0
```

## fanotify Tuning (janusd)

### Key Parameters

fanotify has fewer tunable parameters but is affected by:

| Parameter | Default | Location | Purpose |
|-----------|---------|----------|---------|
| `max_user_watches` | Varies | Kernel compile-time | Max fanotify marks |
| Response timeout | 5s | Hardcoded | Time to respond to permission events |

### Permission Event Latency

When using enforcement mode (`FAN_CLASS_CONTENT`), fanotify blocks processes until janusd responds. If the daemon is slow:

1. All processes accessing monitored paths hang
2. System becomes unresponsive for those paths
3. Potential denial-of-service condition

### Mitigation Strategies

1. **Increase daemon resources**:
   ```yaml
   resources:
     requests:
       cpu: 200m
       memory: 256Mi
     limits:
       cpu: 1000m
       memory: 1Gi
   ```

2. **Reduce monitored paths**: Only monitor critical paths in enforcement mode

3. **Use audit mode for broad monitoring**: Reserve enforcement for specific high-value paths

4. **Implement response timeouts in daemon**: The daemon should have internal timeouts to prevent indefinite hangs

### Kernel Capabilities Required

janusd requires specific capabilities for fanotify:

```yaml
securityContext:
  capabilities:
    add:
      - SYS_ADMIN      # Required for fanotify_init() with FAN_CLASS_CONTENT
      - SYS_PTRACE     # Required for /proc/{pid}/root access
      - DAC_READ_SEARCH # Required for container filesystem access
```

## Memory Considerations

### Watch Memory Usage

Each inotify watch consumes kernel memory:
- ~1KB per watch on 64-bit systems
- 524,288 watches = ~512MB kernel memory

Each fanotify mark consumes:
- ~540 bytes per mark
- Memory scales with number of marked filesystems/directories

### Calculating Requirements

```bash
# Estimate memory for inotify watches
WATCH_COUNT=100000
WATCH_SIZE_KB=1
TOTAL_MB=$((WATCH_COUNT * WATCH_SIZE_KB / 1024))
echo "Estimated inotify memory: ${TOTAL_MB}MB"

# Check current kernel memory usage
cat /proc/meminfo | grep -E 'Slab|SReclaimable|SUnreclaim'
```

## Performance Benchmarking

### Test inotify Throughput

```bash
#!/bin/bash
# inotify-bench.sh - Test event throughput

TEST_DIR=$(mktemp -d)
START=$(date +%s.%N)

for i in $(seq 1 10000); do
  touch $TEST_DIR/file_$i
done

END=$(date +%s.%N)
DURATION=$(echo "$END - $START" | bc)
RATE=$(echo "10000 / $DURATION" | bc)

echo "Created 10000 files in ${DURATION}s"
echo "Rate: ${RATE} files/sec"

rm -rf $TEST_DIR
```

### Monitor Event Processing

```bash
# Watch argusd event processing rate
kubectl logs -n panoptes-system -l app=argusd -f | \
  pv -l -i 5 -r > /dev/null
```

## Troubleshooting

### "Too many open files" Error

```bash
# Increase file descriptor limits
ulimit -n 65536

# Or in systemd service
# /etc/systemd/system/argusd.service.d/limits.conf
[Service]
LimitNOFILE=65536
```

### "No space left on device" from inotify_add_watch

```bash
# Check current usage
cat /proc/sys/fs/inotify/max_user_watches
find /proc/*/fd -lname anon_inode:inotify 2>/dev/null | wc -l

# Increase limit
echo 524288 > /proc/sys/fs/inotify/max_user_watches
```

### Events Being Dropped

Check for `IN_Q_OVERFLOW` in daemon logs:

```bash
kubectl logs -n panoptes-system -l app=argusd | grep -i overflow
```

If overflows are occurring:
1. Increase `max_queued_events`
2. Reduce monitored paths
3. Add ignore patterns for noisy paths
4. Check for runaway processes generating events

### fanotify Permission Denied

```bash
# Check capabilities
kubectl exec -n panoptes-system -l app=janusd -- capsh --print

# Verify fanotify access
kubectl exec -n panoptes-system -l app=janusd -- cat /proc/self/status | grep Cap
```

Required capabilities in hex:
- `CAP_SYS_ADMIN` (21): 0x200000
- `CAP_SYS_PTRACE` (19): 0x80000
- `CAP_DAC_READ_SEARCH` (2): 0x4

## Node-Level Configuration Script

Complete tuning script for worker nodes:

```bash
#!/bin/bash
# /usr/local/bin/panoptes-tune.sh

set -e

echo "Configuring kernel parameters for Panoptes..."

# inotify tuning
sysctl -w fs.inotify.max_queued_events=65536
sysctl -w fs.inotify.max_user_watches=524288
sysctl -w fs.inotify.max_user_instances=256

# File descriptor limits
sysctl -w fs.file-max=2097152

# Make persistent
cat > /etc/sysctl.d/99-panoptes.conf << 'EOF'
fs.inotify.max_queued_events = 65536
fs.inotify.max_user_watches = 524288
fs.inotify.max_user_instances = 256
fs.file-max = 2097152
EOF

echo "Kernel tuning complete. Current values:"
sysctl fs.inotify.max_queued_events
sysctl fs.inotify.max_user_watches
sysctl fs.inotify.max_user_instances
```

## Related Documentation

- [Attack Surface Analysis](../security/attack-surface-analysis.md) - Queue overflow attack details
- [Enabling Enforcement](../guides/enabling-enforcement.md) - fanotify enforcement configuration
- [Advanced Hardening](../security/advanced-hardening.md) - High-security environment tuning
