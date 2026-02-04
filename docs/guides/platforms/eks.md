# Panoptes on Amazon Elastic Kubernetes Service (EKS)

## Supported Configurations

| Node Type | Argus (inotify) | Janus (fanotify) | Notes |
|-----------|----------------|-------------------|-------|
| EC2 (Amazon Linux 2) | Yes | Yes | Default, recommended |
| EC2 (Bottlerocket) | Yes | Yes | Read-only root; see notes below |
| EC2 (Ubuntu) | Yes | Yes | Full Linux kernel access |
| Fargate | No | No | No host access, no DaemonSets |
| Graviton (ARM) | Yes | Yes | ARM64 images required |

## Prerequisites

- EKS cluster 1.28+ with EC2 node groups (not Fargate)
- `aws` and `kubectl` CLI configured
- Node groups with sufficient instance size (t3.medium minimum)

## Deployment

### Helm Install

```bash
helm install panoptes ./packs/panoptes/charts/panoptes \
  --namespace panoptes-system --create-namespace \
  --set global.cluster.name="eks-prod" \
  --set global.cluster.environment="production" \
  --set global.cluster.region="us-east-1"
```

### IAM Roles for Service Accounts (IRSA)

If using IRSA for fine-grained IAM:

```yaml
controller:
  serviceAccount:
    annotations:
      eks.amazonaws.com/role-arn: arn:aws:iam::ACCOUNT_ID:role/panoptes-role
```

## Bottlerocket Nodes

Bottlerocket is a minimal, container-focused OS from AWS. Key considerations:

### Read-Only Root Filesystem

Bottlerocket mounts most paths as read-only. This is compatible with Panoptes since we mount `/host` read-only.

### containerd Socket Path

Bottlerocket uses containerd at the standard path. Auto-detection works.

### API Server for Configuration

Bottlerocket doesn't use SSH. To tune kernel parameters, use the Bottlerocket API:

```toml
# In user data or via API
[settings.kernel.sysctl]
"fs.inotify.max_user_watches" = "1048576"
"fs.inotify.max_user_instances" = "8192"
```

## Fargate Limitations

EKS Fargate does not support Panoptes because:

- No DaemonSet support (Fargate runs each pod in its own micro-VM)
- No host filesystem access
- No `hostPID` namespace sharing

Deploy Panoptes to EC2 managed node groups only.

## Security Groups for Pods

If you use [Security Groups for Pods](https://docs.aws.amazon.com/eks/latest/userguide/security-groups-for-pods.html), ensure the Panoptes daemons can communicate with the controller:

| Source | Destination | Port | Protocol | Purpose |
|--------|------------|------|----------|---------|
| Controller | Daemon | 50051 (Argus), 50052 (Janus) | TCP | gRPC |
| Prometheus | Controller, Daemon | 8080 | TCP | Metrics |

The built-in NetworkPolicy templates handle this within the cluster. Security Groups are only needed if you enforce them at the ENI level.

## Amazon VPC CNI

The default VPC CNI works with Panoptes without additional configuration. If using Calico or Cilium as a CNI plugin, ensure NetworkPolicy enforcement is compatible with Panoptes's policy templates.

## EKS Pod Identity

For EKS clusters using the newer Pod Identity feature (instead of IRSA):

```yaml
controller:
  serviceAccount:
    annotations:
      eks.amazonaws.com/pod-identity-association-role-arn: arn:aws:iam::ACCOUNT_ID:role/panoptes-role
```

## CloudWatch Integration

To forward Panoptes logs to CloudWatch:

1. Install the AWS for Fluent Bit DaemonSet
2. Panoptes daemons log to stdout in structured JSON
3. Logs are automatically collected from the container runtime

For metrics, use Amazon Managed Prometheus with the ServiceMonitor resources or the CloudWatch agent with Prometheus remote write.

## Troubleshooting

### DaemonSet pods not scheduling on all nodes

Check for Fargate profiles that may intercept the pod scheduling:

```bash
aws eks describe-fargate-profile --cluster-name CLUSTER \
  --fargate-profile-name PROFILE
```

Ensure the `panoptes-system` namespace is not matched by any Fargate profile.

### Node group capacity

Verify instances have enough resources. Use at least `t3.medium` for nodes running the full stack.

### Kernel version check

```bash
kubectl debug node/NODE_NAME -it --image=busybox -- uname -r
```

Ensure the kernel is 5.x+ (standard on AL2 and Bottlerocket).
