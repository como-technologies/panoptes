# Prometheus Monitoring for Panoptes

This directory contains Prometheus alerting rules for monitoring Panoptes deployments.

## Files

| File | Purpose |
|------|---------|
| [prometheus-alerts.yaml](./prometheus-alerts.yaml) | PrometheusRule CRD with alerting rules |

## Alert Categories

### inotify Alerts (argusd)

| Alert | Severity | Condition |
|-------|----------|-----------|
| `PanoptesInotifyQueueOverflow` | Critical | Queue overflow detected - events lost |
| `PanoptesInotifyWatchLimitApproaching` | Warning | >80% of watch limit used |
| `PanoptesHighEventRate` | Warning | >1000 events/sec for 5m |

### JanusGuard Alerts (janusd)

| Alert | Severity | Condition |
|-------|----------|-----------|
| `PanoptesCriticalPathDenied` | Critical | Access to critical path denied |
| `PanoptesHighDenyRate` | Warning | >10 denies/sec for 5m |
| `PanoptesCredentialAccessAttempt` | Warning | Any credential file access |
| `PanoptesContainerEscapeAttempt` | Critical | Runtime socket access denied |

### Health Alerts

| Alert | Severity | Condition |
|-------|----------|-----------|
| `PanoptesArgusdDown` | Critical | Daemon not responding for 2m |
| `PanoptesJanusdDown` | Critical | Daemon not responding for 2m |
| `PanoptesDaemonRestarted` | Warning | Daemon restart detected |
| `PanoptesDaemonHighMemory` | Warning | >512MB memory usage |

### Compliance Alerts

| Alert | Severity | Condition |
|-------|----------|-----------|
| `PanoptesAuditLogTampering` | Warning | Log file modified/deleted |
| `PanoptesSystemBinaryModified` | Critical | System binary changed |
| `PanoptesStagingAreaActivity` | Warning | >10 events in staging areas |

## Installation

### With Prometheus Operator

```bash
kubectl apply -f prometheus-alerts.yaml
```

### Standalone Prometheus

Extract the `groups` section and add to your Prometheus rules configuration:

```yaml
# /etc/prometheus/rules/panoptes.yml
groups:
  - name: panoptes-inotify
    rules:
      # ... copy rules from prometheus-alerts.yaml
```

Then reload Prometheus:
```bash
curl -X POST http://localhost:9090/-/reload
```

## Required Metrics

These alerts require the following metrics to be exported by the daemons:

### argusd metrics
- `panoptes_argus_inotify_queue_overflow_total` - Counter of queue overflows
- `panoptes_argus_inotify_watches_total` - Current watch count
- `panoptes_argus_inotify_max_watches` - Maximum watches configured
- `panoptes_argus_events_total` - Counter of events by type

### janusd metrics
- `panoptes_janus_events_total` - Counter of events with labels
- `up{job="janusd"}` - Standard scrape target health

## Customization

### Adjusting Thresholds

Modify threshold values based on your environment:

```yaml
# Lower threshold for high-security environments
- alert: PanoptesHighEventRate
  expr: rate(panoptes_argus_events_total[5m]) > 500  # Changed from 1000
```

### Adding Labels

Add custom labels for routing:

```yaml
labels:
  severity: critical
  team: security  # Route to security team
  environment: production
```

### Integration with Alertmanager

Configure Alertmanager routing:

```yaml
# alertmanager.yml
route:
  routes:
    - match:
        component: argusd
      receiver: platform-team
    - match:
        severity: critical
        component: janusd
      receiver: security-team-pagerduty
```

## Related Documentation

- [Kernel Tuning](../../docs/operations/kernel-tuning.md) - Tuning inotify limits
- [Enabling Enforcement](../../docs/guides/enabling-enforcement.md) - JanusGuard enforcement
- [Attack Surface Analysis](../../docs/security/attack-surface-analysis.md) - Security analysis
