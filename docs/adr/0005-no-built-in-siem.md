# ADR-0005: No Built-in SIEM or Log Aggregation

## Status

Accepted

## Context

Security monitoring tools often evolve into full platforms with built-in log storage, dashboards, alerting engines, and correlation capabilities. We needed to decide Panoptes's boundary:

1. **Build a SIEM** — Store events, provide search, build correlation rules, create dashboards
2. **Export to existing tools** — Generate events and forward them to the user's existing observability stack
3. **Hybrid** — Minimal built-in storage with export capabilities

## Decision

We chose **export to existing tools** — Panoptes generates events and exports them via Prometheus metrics, structured logs, and webhook sinks. We do not build event storage, search, or correlation.

## Rationale

**Why no built-in SIEM:**

- **Every enterprise already has one**: Splunk, Elastic, Datadog, Sumo Logic, Chronicle, Sentinel — organizations have made significant investments in their observability stacks. They don't want another event store to manage, secure, back up, and pay for.
- **Composability > completeness**: The Unix philosophy applied to security tooling. Panoptes detects file changes and controls access. Prometheus stores metrics. Loki stores logs. AlertManager sends alerts. Grafana visualizes. Each tool does its job well.
- **Operational simplicity**: A SIEM requires storage management, retention policies, backup, HA, capacity planning, and access control. That's an entire operations burden on top of the security monitoring itself.
- **Avoid lock-in**: If Panoptes stored events in a proprietary format, migrating to a different tool would require data export tooling. By exporting in standard formats (JSON logs, Prometheus metrics, CloudEvents), users can switch tools freely.
- **Focus**: Building a competitive SIEM would consume all development resources and produce an inferior product compared to dedicated tools with billions in investment.

**How we export instead:**

| Channel | Format | Use Case |
|---------|--------|----------|
| Structured logs | JSON via slog | Ingest into any log aggregator (Loki, Elastic, Splunk) |
| Prometheus metrics | OpenMetrics | Event counts, rates, latencies, error tracking |
| ServiceMonitor CRDs | Prometheus Operator | Auto-discovery by Prometheus |
| PrometheusRule CRDs | AlertManager | Pre-built alert rules for critical events |
| Webhook sink | HTTP POST / CloudEvents | Direct SIEM integration (Splunk HEC, Elastic, custom) |
| Grafana dashboards | JSON models | Pre-built visualization templates |

**The dashboard (Panoptes Eye) is an exception:**

Panoptes Eye provides real-time event viewing and CRD management — it's an operational tool, not a SIEM. It does not store events long-term, does not provide search across historical data, and does not replace a proper observability stack.

## Consequences

- Users must have an existing observability stack (Prometheus + Grafana at minimum)
- Event retention is the user's responsibility, configured in their log aggregator
- Compliance evidence export (audit reports) requires the user's SIEM/log tool, not Panoptes
- We ship pre-built integrations (AlertManager rules, Grafana dashboards, webhook configs) to minimize setup friction
- The webhook event export (ADR pending) enables direct SIEM integration without log scraping
