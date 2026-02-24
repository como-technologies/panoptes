# Panoptes Integration Examples

Ready-to-use integration configurations for connecting Panoptes to your existing security and observability stack.

## Kyverno Auto-Labeling

Automatically label pods for Panoptes compliance monitoring based on namespace annotations. No manual pod labeling required.

- [PCI-DSS auto-label](kyverno/auto-label-pci.yaml) -- Label pods in `payment-*` namespaces
- [HIPAA auto-label](kyverno/auto-label-hipaa.yaml) -- Label pods in `health-*` namespaces

**Usage:**
```bash
kubectl apply -f examples/integrations/kyverno/auto-label-pci.yaml
# Now any pod in namespaces matching "payment-*" gets pci-dss/scope=in-scope automatically
```

## AlertManager (Slack / PagerDuty)

Configure AlertManager to route Panoptes alerts to Slack or PagerDuty.

- [Slack config](alertmanager/slack-config.yaml) -- Route critical file integrity alerts to Slack
- [PagerDuty config](alertmanager/pagerduty-config.yaml) -- Page on-call for access control violations

**Prerequisites:** Prometheus + AlertManager deployed (e.g., via kube-prometheus-stack).
