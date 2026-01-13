# Panoptes Webhook Deployment Guide

This guide covers deploying the Panoptes init container injection webhooks for production use. These webhooks ensure that file monitoring (Argus) and access auditing (Janus) protection is active **before** your application containers start.

---

## Overview

### What the Webhooks Do

Panoptes includes two mutating admission webhooks:

| Webhook | Operator | Purpose |
|---------|----------|---------|
| `watcher-injector` | argus-operator | Injects `watcher-wait` init container into pods matching ArgusWatcher selectors |
| `guard-injector` | janus-operator | Injects `guard-wait` init container into pods matching JanusGuard selectors |

### Why Use Webhooks?

Without webhooks, there's a race condition:
1. Pod starts
2. Application begins accessing files
3. Daemon (argusd/janusd) detects the pod and sets up monitoring
4. **Gap**: File access between steps 2-3 is unmonitored

With webhooks:
1. Pod is created
2. Webhook injects init container
3. Init container blocks until daemon confirms protection is active
4. Main container starts - **all file access is monitored from the start**

### When to Enable Webhooks

| Scenario | Recommendation |
|----------|---------------|
| Local development | Optional - simpler without |
| Testing/staging | Recommended |
| Production | **Required** for compliance |
| Security-critical workloads | **Required** |

---

## Prerequisites

### Required Components

1. **Kubernetes cluster** (1.28+)
2. **Panoptes operators deployed** (argus-operator, janus-operator)
3. **Panoptes daemons running** (argusd, janusd DaemonSets)
4. **TLS certificates** for webhook endpoints (see options below)

### TLS Certificate Options

Webhooks require TLS. Choose one approach:

| Approach | Complexity | Use Case |
|----------|------------|----------|
| **cert-manager** | Medium | Production (recommended) |
| **Self-signed certificates** | Low | Testing, air-gapped environments |
| **External CA** | High | Enterprise PKI integration |

---

## Option 1: Deploy with cert-manager (Recommended)

### Step 1: Install cert-manager

```bash
# Install cert-manager
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.14.0/cert-manager.yaml

# Wait for cert-manager to be ready
kubectl wait --for=condition=Available deployment --all -n cert-manager --timeout=120s
```

### Step 2: Create Certificate Issuers

```bash
# Create self-signed issuer for webhook certs
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: panoptes-selfsigned-issuer
spec:
  selfSigned: {}
---
# CA Certificate for Panoptes
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: panoptes-ca
  namespace: panoptes-system
spec:
  isCA: true
  commonName: panoptes-ca
  secretName: panoptes-ca-secret
  duration: 87600h # 10 years
  privateKey:
    algorithm: ECDSA
    size: 256
  issuerRef:
    name: panoptes-selfsigned-issuer
    kind: ClusterIssuer
---
# Issuer using the CA
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: panoptes-ca-issuer
  namespace: panoptes-system
spec:
  ca:
    secretName: panoptes-ca-secret
EOF
```

### Step 3: Create Webhook Certificates

```bash
# Argus webhook certificate
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: argus-webhook-cert
  namespace: panoptes-system
spec:
  secretName: argus-webhook-tls
  duration: 8760h # 1 year
  renewBefore: 720h # 30 days
  dnsNames:
    - argus-operator-webhook-service
    - argus-operator-webhook-service.panoptes-system
    - argus-operator-webhook-service.panoptes-system.svc
    - argus-operator-webhook-service.panoptes-system.svc.cluster.local
  issuerRef:
    name: panoptes-ca-issuer
    kind: Issuer
---
# Janus webhook certificate
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: janus-webhook-cert
  namespace: panoptes-system
spec:
  secretName: janus-webhook-tls
  duration: 8760h # 1 year
  renewBefore: 720h # 30 days
  dnsNames:
    - janus-operator-webhook-service
    - janus-operator-webhook-service.panoptes-system
    - janus-operator-webhook-service.panoptes-system.svc
    - janus-operator-webhook-service.panoptes-system.svc.cluster.local
  issuerRef:
    name: panoptes-ca-issuer
    kind: Issuer
EOF

# Wait for certificates to be ready
kubectl wait --for=condition=Ready certificate/argus-webhook-cert -n panoptes-system --timeout=60s
kubectl wait --for=condition=Ready certificate/janus-webhook-cert -n panoptes-system --timeout=60s
```

### Step 4: Update Operator Deployments

Patch the operators to mount certificates and enable webhooks:

```bash
# Patch argus-operator
kubectl patch deployment argus-operator-controller-manager -n panoptes-system --type='json' -p='[
  {
    "op": "add",
    "path": "/spec/template/spec/volumes/-",
    "value": {
      "name": "webhook-certs",
      "secret": {
        "secretName": "argus-webhook-tls"
      }
    }
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/volumeMounts/-",
    "value": {
      "name": "webhook-certs",
      "mountPath": "/tmp/k8s-webhook-server/serving-certs",
      "readOnly": true
    }
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/args/-",
    "value": "--enable-webhook=true"
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/env/-",
    "value": {
      "name": "WATCHER_WAIT_IMAGE",
      "value": "panoptes/watcher-wait:latest"
    }
  }
]'

# Patch janus-operator
kubectl patch deployment janus-operator-controller-manager -n panoptes-system --type='json' -p='[
  {
    "op": "add",
    "path": "/spec/template/spec/volumes/-",
    "value": {
      "name": "webhook-certs",
      "secret": {
        "secretName": "janus-webhook-tls"
      }
    }
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/volumeMounts/-",
    "value": {
      "name": "webhook-certs",
      "mountPath": "/tmp/k8s-webhook-server/serving-certs",
      "readOnly": true
    }
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/args/-",
    "value": "--enable-webhook=true"
  },
  {
    "op": "add",
    "path": "/spec/template/spec/containers/0/env/-",
    "value": {
      "name": "GUARD_WAIT_IMAGE",
      "value": "panoptes/guard-wait:latest"
    }
  }
]'
```

### Step 5: Deploy MutatingWebhookConfigurations

```bash
# Get the CA bundle from cert-manager
CA_BUNDLE=$(kubectl get secret panoptes-ca-secret -n panoptes-system -o jsonpath='{.data.ca\.crt}')

# Deploy Argus webhook configuration
cat <<EOF | kubectl apply -f -
apiVersion: admissionregistration.k8s.io/v1
kind: MutatingWebhookConfiguration
metadata:
  name: argus-watcher-injector
  annotations:
    cert-manager.io/inject-ca-from: panoptes-system/argus-webhook-cert
webhooks:
  - name: watcher-injector.argus.panoptes.io
    clientConfig:
      service:
        name: argus-operator-webhook-service
        namespace: panoptes-system
        path: /mutate-pod
        port: 443
      caBundle: ${CA_BUNDLE}
    rules:
      - operations: ["CREATE"]
        apiGroups: [""]
        apiVersions: ["v1"]
        resources: ["pods"]
    namespaceSelector:
      matchLabels:
        argus.panoptes.io/watcher-injection: enabled
    failurePolicy: Fail
    sideEffects: None
    admissionReviewVersions: ["v1"]
    timeoutSeconds: 10
EOF

# Deploy Janus webhook configuration
cat <<EOF | kubectl apply -f -
apiVersion: admissionregistration.k8s.io/v1
kind: MutatingWebhookConfiguration
metadata:
  name: janus-guard-injector
  annotations:
    cert-manager.io/inject-ca-from: panoptes-system/janus-webhook-cert
webhooks:
  - name: guard-injector.janus.panoptes.io
    clientConfig:
      service:
        name: janus-operator-webhook-service
        namespace: panoptes-system
        path: /mutate-pod
        port: 443
      caBundle: ${CA_BUNDLE}
    rules:
      - operations: ["CREATE"]
        apiGroups: [""]
        apiVersions: ["v1"]
        resources: ["pods"]
    namespaceSelector:
      matchLabels:
        janus.panoptes.io/guard-injection: enabled
    failurePolicy: Fail
    sideEffects: None
    admissionReviewVersions: ["v1"]
    timeoutSeconds: 10
EOF
```

### Step 6: Deploy Webhook Services

```bash
# Argus webhook service
kubectl apply -f - <<EOF
apiVersion: v1
kind: Service
metadata:
  name: argus-operator-webhook-service
  namespace: panoptes-system
spec:
  ports:
    - port: 443
      targetPort: 9443
      protocol: TCP
  selector:
    app.kubernetes.io/name: argus-operator
EOF

# Janus webhook service
kubectl apply -f - <<EOF
apiVersion: v1
kind: Service
metadata:
  name: janus-operator-webhook-service
  namespace: panoptes-system
spec:
  ports:
    - port: 443
      targetPort: 9443
      protocol: TCP
  selector:
    app.kubernetes.io/name: janus-operator
EOF
```

### Step 7: Enable Webhook Injection on Namespaces

```bash
# Enable Argus watcher injection on a namespace
kubectl label namespace default argus.panoptes.io/watcher-injection=enabled

# Enable Janus guard injection on a namespace
kubectl label namespace default janus.panoptes.io/guard-injection=enabled

# Verify labels
kubectl get namespace default --show-labels
```

---

## Option 2: Deploy with Self-Signed Certificates

For testing or air-gapped environments without cert-manager.

### Step 1: Generate Self-Signed Certificates

```bash
#!/bin/bash
# generate-webhook-certs.sh

NAMESPACE="panoptes-system"
CERT_DIR="/tmp/panoptes-certs"
mkdir -p "${CERT_DIR}"

# Generate CA
openssl genrsa -out "${CERT_DIR}/ca.key" 2048
openssl req -x509 -new -nodes -key "${CERT_DIR}/ca.key" \
    -subj "/CN=panoptes-ca" -days 3650 -out "${CERT_DIR}/ca.crt"

# Generate Argus webhook cert
cat > "${CERT_DIR}/argus-csr.conf" <<EOF
[req]
req_extensions = v3_req
distinguished_name = req_distinguished_name
[req_distinguished_name]
[v3_req]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
subjectAltName = @alt_names
[alt_names]
DNS.1 = argus-operator-webhook-service
DNS.2 = argus-operator-webhook-service.${NAMESPACE}
DNS.3 = argus-operator-webhook-service.${NAMESPACE}.svc
DNS.4 = argus-operator-webhook-service.${NAMESPACE}.svc.cluster.local
EOF

openssl genrsa -out "${CERT_DIR}/argus-tls.key" 2048
openssl req -new -key "${CERT_DIR}/argus-tls.key" \
    -subj "/CN=argus-operator-webhook-service.${NAMESPACE}.svc" \
    -config "${CERT_DIR}/argus-csr.conf" \
    -out "${CERT_DIR}/argus-tls.csr"
openssl x509 -req -in "${CERT_DIR}/argus-tls.csr" \
    -CA "${CERT_DIR}/ca.crt" -CAkey "${CERT_DIR}/ca.key" \
    -CAcreateserial -out "${CERT_DIR}/argus-tls.crt" \
    -days 365 -extensions v3_req -extfile "${CERT_DIR}/argus-csr.conf"

# Generate Janus webhook cert
cat > "${CERT_DIR}/janus-csr.conf" <<EOF
[req]
req_extensions = v3_req
distinguished_name = req_distinguished_name
[req_distinguished_name]
[v3_req]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
subjectAltName = @alt_names
[alt_names]
DNS.1 = janus-operator-webhook-service
DNS.2 = janus-operator-webhook-service.${NAMESPACE}
DNS.3 = janus-operator-webhook-service.${NAMESPACE}.svc
DNS.4 = janus-operator-webhook-service.${NAMESPACE}.svc.cluster.local
EOF

openssl genrsa -out "${CERT_DIR}/janus-tls.key" 2048
openssl req -new -key "${CERT_DIR}/janus-tls.key" \
    -subj "/CN=janus-operator-webhook-service.${NAMESPACE}.svc" \
    -config "${CERT_DIR}/janus-csr.conf" \
    -out "${CERT_DIR}/janus-tls.csr"
openssl x509 -req -in "${CERT_DIR}/janus-tls.csr" \
    -CA "${CERT_DIR}/ca.crt" -CAkey "${CERT_DIR}/ca.key" \
    -CAcreateserial -out "${CERT_DIR}/janus-tls.crt" \
    -days 365 -extensions v3_req -extfile "${CERT_DIR}/janus-csr.conf"

echo "Certificates generated in ${CERT_DIR}"
```

### Step 2: Create Kubernetes Secrets

```bash
CERT_DIR="/tmp/panoptes-certs"
NAMESPACE="panoptes-system"

# Create Argus webhook secret
kubectl create secret tls argus-webhook-tls \
    --cert="${CERT_DIR}/argus-tls.crt" \
    --key="${CERT_DIR}/argus-tls.key" \
    -n "${NAMESPACE}"

# Create Janus webhook secret
kubectl create secret tls janus-webhook-tls \
    --cert="${CERT_DIR}/janus-tls.crt" \
    --key="${CERT_DIR}/janus-tls.key" \
    -n "${NAMESPACE}"

# Store CA bundle for webhook configurations
CA_BUNDLE=$(cat "${CERT_DIR}/ca.crt" | base64 | tr -d '\n')
echo "CA_BUNDLE=${CA_BUNDLE}"
```

### Step 3: Continue from Step 4 Above

Follow Steps 4-7 from Option 1, using the CA_BUNDLE from Step 2.

---

## Configuration Reference

### Operator Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--enable-webhook` | `false` | Enable the init container injection webhook |
| `--webhook-cert-path` | `/tmp/k8s-webhook-server/serving-certs` | Directory containing TLS certs |
| `--webhook-cert-name` | `tls.crt` | Certificate filename |
| `--webhook-cert-key` | `tls.key` | Private key filename |

### Environment Variables

| Variable | Operator | Description |
|----------|----------|-------------|
| `WATCHER_WAIT_IMAGE` | argus-operator | Image for watcher-wait init container |
| `ARGUSD_ADDRESS` | argus-operator | gRPC address of argusd daemon |
| `WATCHER_MAX_WAIT_SECS` | argus-operator | Max seconds to wait for protection |
| `GUARD_WAIT_IMAGE` | janus-operator | Image for guard-wait init container |
| `JANUSD_ADDRESS` | janus-operator | gRPC address of janusd daemon |
| `GUARD_MAX_WAIT_SECS` | janus-operator | Max seconds to wait for protection |

### Namespace Labels

| Label | Value | Effect |
|-------|-------|--------|
| `argus.panoptes.io/watcher-injection` | `enabled` | Enable ArgusWatcher init container injection |
| `janus.panoptes.io/guard-injection` | `enabled` | Enable JanusGuard init container injection |

### Pod Annotations

| Annotation | Value | Effect |
|------------|-------|--------|
| `argus.panoptes.io/inject` | `false` | Disable injection for this pod |
| `janus.panoptes.io/inject` | `false` | Disable injection for this pod |

---

## Verification

### Check Webhook Registration

```bash
# List webhook configurations
kubectl get mutatingwebhookconfigurations | grep panoptes

# Describe webhooks
kubectl describe mutatingwebhookconfiguration argus-watcher-injector
kubectl describe mutatingwebhookconfiguration janus-guard-injector
```

### Check Operator Logs

```bash
# Argus operator should show webhook registered
kubectl logs -n panoptes-system deployment/argus-operator-controller-manager | grep webhook

# Janus operator should show webhook registered
kubectl logs -n panoptes-system deployment/janus-operator-controller-manager | grep webhook
```

### Test Injection

```bash
# Create a test pod in an injection-enabled namespace
kubectl run test-injection --image=nginx:alpine -n default

# Check if init container was injected
kubectl get pod test-injection -n default -o jsonpath='{.spec.initContainers[*].name}'
# Should show: wait-for-watcher wait-for-guard (if both enabled)

# Check pod annotations
kubectl get pod test-injection -n default -o jsonpath='{.metadata.annotations}' | jq .

# Cleanup
kubectl delete pod test-injection -n default
```

---

## Troubleshooting

### Webhook Not Injecting Init Containers

1. **Check namespace labels**:
   ```bash
   kubectl get namespace <ns> --show-labels | grep injection
   ```

2. **Check webhook configuration**:
   ```bash
   kubectl get mutatingwebhookconfiguration argus-watcher-injector -o yaml
   ```

3. **Check operator logs**:
   ```bash
   kubectl logs -n panoptes-system deployment/argus-operator-controller-manager
   ```

4. **Check if webhook is enabled**:
   ```bash
   kubectl get deployment argus-operator-controller-manager -n panoptes-system \
       -o jsonpath='{.spec.template.spec.containers[0].args}'
   # Should include --enable-webhook=true
   ```

### Certificate Issues

1. **Certificate not found**:
   ```bash
   kubectl get secret argus-webhook-tls -n panoptes-system
   kubectl describe secret argus-webhook-tls -n panoptes-system
   ```

2. **Certificate expired**:
   ```bash
   kubectl get certificate -n panoptes-system
   # Check READY status and EXPIRATION
   ```

3. **CA bundle mismatch**:
   ```bash
   # Get CA from secret
   kubectl get secret argus-webhook-tls -n panoptes-system -o jsonpath='{.data.ca\.crt}' | base64 -d

   # Compare with webhook configuration
   kubectl get mutatingwebhookconfiguration argus-watcher-injector -o jsonpath='{.webhooks[0].clientConfig.caBundle}' | base64 -d
   ```

### Init Container Stuck or Failing

1. **Check init container logs**:
   ```bash
   kubectl logs <pod-name> -c wait-for-watcher
   kubectl logs <pod-name> -c wait-for-guard
   ```

2. **Check daemon connectivity**:
   ```bash
   # Verify daemon pods are running
   kubectl get pods -n panoptes-system -l app=argusd
   kubectl get pods -n panoptes-system -l app=janusd

   # Check daemon service
   kubectl get svc -n panoptes-system | grep -E "(argusd|janusd)"
   ```

3. **Increase wait timeout**:
   ```bash
   kubectl set env deployment/argus-operator-controller-manager -n panoptes-system \
       WATCHER_MAX_WAIT_SECS=60
   ```

### Webhook Causing Pod Creation Failures

1. **Set failure policy to Ignore temporarily**:
   ```bash
   kubectl patch mutatingwebhookconfiguration argus-watcher-injector \
       --type='json' -p='[{"op": "replace", "path": "/webhooks/0/failurePolicy", "value": "Ignore"}]'
   ```

2. **Disable webhook injection for specific pod**:
   ```yaml
   apiVersion: v1
   kind: Pod
   metadata:
     annotations:
       argus.panoptes.io/inject: "false"
       janus.panoptes.io/inject: "false"
   ```

---

## Production Considerations

### High Availability

- Deploy operators with multiple replicas
- Use `failurePolicy: Ignore` if webhook availability is a concern
- Implement proper monitoring and alerting

### Security

- Use short-lived certificates (rotate annually at minimum)
- Restrict webhook service network access
- Audit webhook activity via Kubernetes audit logs

### Performance

- Set appropriate `timeoutSeconds` (default: 10s)
- Monitor webhook latency via operator metrics
- Consider `failurePolicy: Ignore` for non-critical workloads

### Compliance

- Document webhook configuration for auditors
- Ensure init container images are scanned and signed
- Maintain certificate chain documentation

---

*Copyright 2026 Como Technologies, LTD. Licensed under Apache 2.0.*
