# Panoptes Operators Common Library

Shared Go library for Panoptes Kubernetes operators.

## Overview

This package provides common infrastructure shared between the argus-operator and janus-operator. Currently, it contains the generic init container injection webhook framework.

## Components

### Webhook Package (`webhook/`)

Generic mutating admission webhook that injects init containers into pods matching custom resource selectors. This enables the "hardened startup" pattern where pods are blocked from starting until their monitoring/guarding is active.

#### InjectorConfig (`config.go`)

Configuration struct for the generic injector:

```go
type InjectorConfig struct {
    InitContainerName  string  // e.g., "wait-for-watcher", "wait-for-guard"
    DefaultImage       string  // Init container image
    ImageEnvVar        string  // Env var to override image
    DefaultAddress     string  // Daemon gRPC address
    AddressEnvVar      string  // Env var to override address
    MaxWaitSecsEnvVar  string  // Env var for timeout
    DefaultMaxWaitSecs string  // Default timeout value
    DomainPrefix       string  // Annotation/label domain (e.g., "argus.panoptes.io")
    ResourceNameEnvVar string  // Env var name in init container
    WebhookName        string  // Human-readable name for logging
    ResourceTypeName   string  // Resource type for logging (e.g., "ArgusWatcher")
}
```

Helper methods:
- `GetImage()` - Returns image from env var or default
- `GetAddress()` - Returns daemon address from env var or default
- `GetMaxWaitSecs()` - Returns timeout from env var or default
- `AnnotationInject()` - Returns inject enable/disable annotation key
- `AnnotationResourceName()` - Returns resource name annotation key
- `LabelInjected()` - Returns injected label key

#### GenericInjector (`injector.go`)

Mutating webhook handler that:
1. Decodes incoming pod admission requests
2. Checks if injection is disabled via annotation
3. Checks if pod already has the init container
4. Calls `ResourceMatcher` to find matching custom resource
5. Injects init container with appropriate env vars
6. Adds tracking annotations and labels

The `ResourceMatcher` interface must be implemented by the consuming operator:

```go
type ResourceMatcher interface {
    FindMatchingResource(ctx context.Context, pod *corev1.Pod) (resourceName string, err error)
}
```

## Usage

### In argus-operator

```go
import (
    "github.com/como-technologies/panoptes/operators/common/webhook"
    argusv2 "github.com/como-technologies/panoptes/operators/argus-operator/api/v2"
)

// Implement ResourceMatcher for ArgusWatcher
type argusWatcherMatcher struct {
    client client.Client
}

func (m *argusWatcherMatcher) FindMatchingResource(ctx context.Context, pod *corev1.Pod) (string, error) {
    // List ArgusWatchers in namespace, find one whose selector matches pod
    // Return watcher name or "" if no match
}

// Create injector
config := webhook.InjectorConfig{
    InitContainerName:  "wait-for-watcher",
    DefaultImage:       "panoptes/watcher-wait:latest",
    ImageEnvVar:        "WATCHER_WAIT_IMAGE",
    DefaultAddress:     "http://argusd.panoptes-system:50051",
    AddressEnvVar:      "ARGUSD_ADDRESS",
    MaxWaitSecsEnvVar:  "WATCHER_MAX_WAIT_SECS",
    DefaultMaxWaitSecs: "30",
    DomainPrefix:       "argus.panoptes.io",
    ResourceNameEnvVar: "WATCHER_NAME",
    WebhookName:        "watcher-injector",
    ResourceTypeName:   "ArgusWatcher",
}

injector := webhook.NewGenericInjector(mgr.GetClient(), config, &argusWatcherMatcher{client: mgr.GetClient()})
```

### In janus-operator

```go
import (
    "github.com/como-technologies/panoptes/operators/common/webhook"
    janusv2 "github.com/como-technologies/panoptes/operators/janus-operator/api/v2"
)

// Implement ResourceMatcher for JanusGuard
type janusGuardMatcher struct {
    client client.Client
}

func (m *janusGuardMatcher) FindMatchingResource(ctx context.Context, pod *corev1.Pod) (string, error) {
    // List JanusGuards in namespace, find one whose selector matches pod
    // Return guard name or "" if no match
}

// Create injector
config := webhook.InjectorConfig{
    InitContainerName:  "wait-for-guard",
    DefaultImage:       "panoptes/guard-wait:latest",
    ImageEnvVar:        "GUARD_WAIT_IMAGE",
    DefaultAddress:     "http://janusd.panoptes-system:50052",
    AddressEnvVar:      "JANUSD_ADDRESS",
    MaxWaitSecsEnvVar:  "GUARD_MAX_WAIT_SECS",
    DefaultMaxWaitSecs: "30",
    DomainPrefix:       "janus.panoptes.io",
    ResourceNameEnvVar: "GUARD_NAME",
    WebhookName:        "guard-injector",
    ResourceTypeName:   "JanusGuard",
}

injector := webhook.NewGenericInjector(mgr.GetClient(), config, &janusGuardMatcher{client: mgr.GetClient()})
```

## Init Container Behavior

The injected init container:
- Polls the daemon's gRPC endpoint until watches/guards are ready
- Blocks main container startup until protection is active
- Has resource limits: 100m CPU / 32Mi memory (limits), 10m CPU / 8Mi memory (requests)
- Receives environment variables:
  - `NAMESPACE` - Pod namespace (from fieldRef)
  - `POD_NAME` - Pod name (from fieldRef)
  - Resource name env var (e.g., `WATCHER_NAME`, `GUARD_NAME`)
  - Daemon address env var (e.g., `ARGUSD_ADDRESS`, `JANUSD_ADDRESS`)
  - `MAX_WAIT_SECS` - Timeout for readiness polling

## Annotations and Labels

### Annotations
- `{domain}/inject` - Set to `"false"` to disable injection for a pod
- `{domain}/{resource}-name` - Added by webhook to track which resource matched

### Labels
- `{domain}/{init-container-name}-injected` - Set to `"true"` when init container injected

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
