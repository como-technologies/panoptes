# Operations Documentation

This directory contains operational guides for running Panoptes in production environments.

## Documents

| Document | Purpose | Audience |
|----------|---------|----------|
| [Kernel Tuning](./kernel-tuning.md) | Linux kernel parameter optimization | Platform engineers, SREs |

## Quick Reference

### inotify Tuning (argusd)

```bash
# Recommended production settings
sysctl -w fs.inotify.max_queued_events=65536
sysctl -w fs.inotify.max_user_watches=524288
sysctl -w fs.inotify.max_user_instances=256
```

### Required Capabilities (janusd)

```yaml
capabilities:
  add:
    - SYS_ADMIN      # Required: fanotify initialization
    - SYS_PTRACE     # Required: /proc access for container PIDs
    - DAC_READ_SEARCH  # Optional: container filesystem traversal
    - AUDIT_WRITE    # Optional: kernel audit log integration
```

## Related Documentation

- [Enabling Enforcement](../guides/enabling-enforcement.md) - Safely enable blocking mode
- [Security Documentation](../security/README.md) - Attack surface and hardening
