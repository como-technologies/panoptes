/*
Copyright 2026 Como Technologies, LTD.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

// Package webhook provides a generic init container injector for Panoptes operators.
//
// This package abstracts the common logic for injecting init containers into pods
// that match a selector. Both ArgusWatcher and JanusGuard use this pattern to
// ensure protection is active before the main container starts.
package webhook

import (
	"fmt"
	"os"
)

// InjectorConfig holds configuration for the generic init container injector.
type InjectorConfig struct {
	// InitContainerName is the name of the injected init container
	// e.g., "wait-for-watcher" or "wait-for-guard"
	InitContainerName string

	// DefaultImage is the default container image for the init container
	// e.g., "panoptes/watcher-wait:latest"
	DefaultImage string

	// ImageEnvVar is the environment variable to override the image
	// e.g., "WATCHER_WAIT_IMAGE"
	ImageEnvVar string

	// DefaultAddress is the default daemon gRPC service address
	// e.g., "http://argusd.panoptes-system:50051"
	DefaultAddress string

	// AddressEnvVar is the environment variable for the daemon address
	// e.g., "ARGUSD_ADDRESS"
	AddressEnvVar string

	// MaxWaitSecsEnvVar is the environment variable for max wait timeout
	// e.g., "WATCHER_MAX_WAIT_SECS"
	MaxWaitSecsEnvVar string

	// DefaultMaxWaitSecs is the default timeout for waiting for readiness
	DefaultMaxWaitSecs string

	// DomainPrefix is the annotation/label domain prefix
	// e.g., "argus.panoptes.io" or "janus.panoptes.io"
	DomainPrefix string

	// ResourceNameEnvVar is the env var name for the resource name in the init container
	// e.g., "WATCHER_NAME" or "GUARD_NAME"
	ResourceNameEnvVar string

	// WebhookName is a human-readable name for logging
	// e.g., "watcher-injector" or "guard-injector"
	WebhookName string

	// ResourceTypeName is the name of the resource type for logging
	// e.g., "ArgusWatcher" or "JanusGuard"
	ResourceTypeName string
}

// GetImage returns the image to use, checking the environment variable first.
func (c *InjectorConfig) GetImage() string {
	if v := os.Getenv(c.ImageEnvVar); v != "" {
		return v
	}
	return c.DefaultImage
}

// GetAddress returns the daemon address, checking the environment variable first.
func (c *InjectorConfig) GetAddress() string {
	if v := os.Getenv(c.AddressEnvVar); v != "" {
		return v
	}
	return c.DefaultAddress
}

// GetMaxWaitSecs returns the max wait timeout, checking the environment variable first.
func (c *InjectorConfig) GetMaxWaitSecs() string {
	if v := os.Getenv(c.MaxWaitSecsEnvVar); v != "" {
		return v
	}
	return c.DefaultMaxWaitSecs
}

// AnnotationInject returns the annotation key for enabling/disabling injection.
// e.g., "argus.panoptes.io/inject"
func (c *InjectorConfig) AnnotationInject() string {
	return fmt.Sprintf("%s/inject", c.DomainPrefix)
}

// AnnotationResourceName returns the annotation key for the resource name.
// e.g., "argus.panoptes.io/watcher-name"
func (c *InjectorConfig) AnnotationResourceName() string {
	// Extract the resource type from InitContainerName (e.g., "wait-for-watcher" -> "watcher")
	return fmt.Sprintf("%s/%s-name", c.DomainPrefix, c.InitContainerName[9:]) // skip "wait-for-"
}

// LabelInjected returns the label key indicating injection occurred.
// e.g., "argus.panoptes.io/watcher-wait-injected"
func (c *InjectorConfig) LabelInjected() string {
	return fmt.Sprintf("%s/%s-injected", c.DomainPrefix, c.InitContainerName)
}
