# Monitoring and Alerting

This guide covers Prometheus metrics, Grafana dashboards, and alerting for Panoptes.

## Prometheus Metrics

Both operators expose Prometheus metrics on port 8080.

### Argus Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `argus_controller_watched_pods_total` | Gauge | Pods with active watches (by watcher, namespace) |
| `argus_controller_observable_pods_total` | Gauge | Pods matching selector (by watcher, namespace) |
| `argus_controller_watch_descriptors_total` | Gauge | Active inotify descriptors (by watcher, namespace) |
| `argus_controller_events_detected_total` | Counter | File events detected (by watcher, namespace, type) |
| `argus_controller_reconcile_total` | Counter | Controller reconciliations (by result) |
| `argus_controller_reconcile_duration_seconds` | Histogram | Reconciliation latency |
| `argus_controller_watcher_condition` | Gauge | Watcher condition status (by watcher, namespace, condition) |

### Janus Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `janus_controller_guarded_pods_total` | Gauge | Pods with active guards (by guard, namespace) |
| `janus_controller_observable_pods_total` | Gauge | Pods matching selector (by guard, namespace) |
| `janus_controller_denied_access_total` | Counter | Blocked access attempts (by guard, namespace, path) |
| `janus_controller_allowed_access_total` | Counter | Allowed access events (by guard, namespace) |
| `janus_controller_audited_access_total` | Counter | Audited access events (by guard, namespace) |
| `janus_controller_reconcile_total` | Counter | Controller reconciliations (by result) |
| `janus_controller_reconcile_duration_seconds` | Histogram | Reconciliation latency |
| `janus_controller_guard_condition` | Gauge | Guard condition status (by guard, namespace, condition) |

## ServiceMonitor Setup

If you're using Prometheus Operator, create ServiceMonitors:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: argus-operator
  namespace: panoptes-system
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: argus-operator
  endpoints:
    - port: metrics
      interval: 30s
---
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: janus-operator
  namespace: panoptes-system
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: janus-operator
  endpoints:
    - port: metrics
      interval: 30s
```

## Grafana Dashboard

Import the Panoptes dashboard from `deploy/monitoring/grafana-dashboard.json` or use the dashboard ID from Grafana.com (coming soon).

The dashboard includes:
- Overview of watched/guarded pods
- Event timeline by type
- Top modified paths
- Denied access breakdown
- Reconciliation performance

## Alerting Rules

### Critical File Modifications

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: argus-alerts
  namespace: panoptes-system
spec:
  groups:
    - name: argus.critical
      rules:
        - alert: CriticalFileModified
          expr: |
            increase(argus_controller_events_detected_total{
              type=~"modify|delete"
            }[5m]) > 0
          for: 0m
          labels:
            severity: critical
          annotations:
            summary: "Critical file modified in {{ $labels.namespace }}"
            description: "Watcher {{ $labels.watcher }} detected {{ $labels.type }} event on {{ $labels.path }}"

        - alert: ArgusWatcherNotReady
          expr: |
            argus_controller_watcher_condition{condition="Ready"} == 0
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: "ArgusWatcher {{ $labels.watcher }} not ready"
            description: "Watcher has been in non-ready state for 5 minutes"
```

### Access Denials

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: janus-alerts
  namespace: panoptes-system
spec:
  groups:
    - name: janus.security
      rules:
        - alert: UnauthorizedAccessAttempt
          expr: |
            increase(janus_controller_denied_access_total[5m]) > 5
          for: 0m
          labels:
            severity: critical
          annotations:
            summary: "Multiple denied access attempts in {{ $labels.namespace }}"
            description: "Guard {{ $labels.guard }} blocked {{ $value }} access attempts"

        - alert: JanusGuardNotReady
          expr: |
            janus_controller_guard_condition{condition="Ready"} == 0
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: "JanusGuard {{ $labels.guard }} not ready"
            description: "Guard has been in non-ready state for 5 minutes"
```

### Operational Alerts

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: panoptes-operational
  namespace: panoptes-system
spec:
  groups:
    - name: panoptes.operational
      rules:
        - alert: HighReconcileLatency
          expr: |
            histogram_quantile(0.99,
              rate(argus_controller_reconcile_duration_seconds_bucket[5m])
            ) > 10
          for: 10m
          labels:
            severity: warning
          annotations:
            summary: "High reconcile latency for Argus controller"
            description: "P99 reconcile latency is {{ $value }}s"

        - alert: WatchDescriptorExhaustion
          expr: |
            argus_controller_watch_descriptors_total > 50000
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: "High inotify watch descriptor count"
            description: "Watcher {{ $labels.watcher }} using {{ $value }} descriptors"
```

## Integration with SIEM

Panoptes events can be forwarded to your SIEM via:

1. **Prometheus → Alertmanager → Webhook**: Forward alerts to SIEM webhook endpoint
2. **Loki/Fluentd**: Collect daemon logs and forward to SIEM
3. **Kernel audit log**: Janus writes to audit log when `audit: true` is set

### Loki Log Queries

```logql
# All file modification events
{app="argusd"} |= "event_type=modify"

# Denied access attempts
{app="janusd"} |= "response=deny"

# Events by namespace
{app="argusd"} | json | namespace="production"
```

## Related Documentation

- [Kernel Tuning](../operations/kernel-tuning.md) - Tune inotify/fanotify limits
- [What to Monitor](what-to-monitor.md) - Recommended paths and patterns
- [Troubleshooting](troubleshooting.md) - Debugging metrics issues
