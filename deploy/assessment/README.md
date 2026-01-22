# Panoptes Compliance Assessment - Kubernetes Job

Run the Panoptes compliance assessment as a Kubernetes Job.

## Quick Start

```bash
# Deploy RBAC (one-time)
kubectl apply -f rbac.yaml

# Run assessment
kubectl apply -f job.yaml

# View results
kubectl logs -f job/panoptes-assess -n panoptes-system

# Clean up
kubectl delete job panoptes-assess -n panoptes-system
```

## Prerequisites

- Kubernetes cluster with Panoptes CRDs installed
- `panoptes-system` namespace exists

Create the namespace if needed:
```bash
kubectl create namespace panoptes-system
```

## Files

| File | Description |
|------|-------------|
| `rbac.yaml` | ServiceAccount and read-only ClusterRole |
| `job.yaml` | Assessment Job definition |

## RBAC Permissions

The assessment Job uses read-only permissions:

```yaml
rules:
  - apiGroups: [""]
    resources: ["pods", "namespaces"]
    verbs: ["get", "list"]
  - apiGroups: ["apps"]
    resources: ["deployments", "replicasets", "daemonsets", "statefulsets"]
    verbs: ["get", "list"]
  - apiGroups: ["argus.como-technologies.io"]
    resources: ["arguswatchers"]
    verbs: ["get", "list"]
  - apiGroups: ["janus.como-technologies.io"]
    resources: ["janusguards"]
    verbs: ["get", "list"]
```

## Customization

### Change target namespace

Edit the Job to scan a specific namespace:

```yaml
env:
  - name: TARGET_NAMESPACE
    value: "production"
```

### Run as CronJob

For scheduled assessments, convert to CronJob:

```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: panoptes-assess
  namespace: panoptes-system
spec:
  schedule: "0 6 * * 1"  # Weekly on Monday at 6 AM
  jobTemplate:
    spec:
      # ... same as job.yaml spec
```

### Output to ConfigMap

To save results for later retrieval:

```yaml
# Add to Job spec
volumes:
  - name: output
    emptyDir: {}

# Add to container
volumeMounts:
  - name: output
    mountPath: /output

# Modify command to save output
command:
  - /bin/bash
  - -c
  - |
    ./assess.sh --output=json > /output/report.json
    kubectl create configmap panoptes-assessment-report \
      --from-file=/output/report.json \
      -n panoptes-system \
      --dry-run=client -o yaml | kubectl apply -f -
```

## Troubleshooting

### Job fails with RBAC errors

Ensure RBAC is applied:
```bash
kubectl apply -f rbac.yaml
kubectl auth can-i list pods --as=system:serviceaccount:panoptes-system:panoptes-assess
```

### CRDs not found

The assessment will work but won't detect existing ArgusWatchers/JanusGuards if CRDs aren't installed:
```bash
kubectl get crd arguswatchers.argus.como-technologies.io
kubectl get crd janusguards.janus.como-technologies.io
```

### Namespace doesn't exist

```bash
kubectl create namespace panoptes-system
```

## Integration

### CI/CD Pipeline

```yaml
# GitLab CI example
compliance-assessment:
  stage: security
  script:
    - kubectl apply -f deploy/assessment/rbac.yaml
    - kubectl delete job panoptes-assess -n panoptes-system --ignore-not-found
    - kubectl apply -f deploy/assessment/job.yaml
    - kubectl wait --for=condition=complete job/panoptes-assess -n panoptes-system --timeout=300s
    - kubectl logs job/panoptes-assess -n panoptes-system
```

### Alerting on Findings

Use a Job that posts results to Slack/webhook:

```yaml
env:
  - name: SLACK_WEBHOOK
    valueFrom:
      secretKeyRef:
        name: slack-webhook
        key: url
```
