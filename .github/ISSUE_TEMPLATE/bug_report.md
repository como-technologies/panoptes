---
name: Bug Report
about: Report a bug in Panoptes
title: '[Bug] '
labels: bug
assignees: ''
---

## Describe the Bug

A clear description of what the bug is.

## To Reproduce

Steps to reproduce:
1.
2.
3.

## Expected Behavior

What you expected to happen.

## Environment

- **Kubernetes version**: (e.g., 1.30)
- **Container runtime**: (containerd / CRI-O)
- **Panoptes version**: (e.g., 2.0.0)
- **Installation method**: (Helm / Kustomize / hack/local-deploy.sh)
- **Platform**: (Kind / EKS / GKE / AKS / other)
- **Linux kernel version**: (`uname -r`)

## Logs

```
# Operator logs
kubectl logs -n panoptes-system deploy/argus-controller

# Daemon logs
kubectl logs -n panoptes-system -l app.kubernetes.io/component=daemon
```

## Additional Context

Any other context, screenshots, or CRD definitions.
