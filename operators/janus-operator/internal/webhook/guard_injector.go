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

// Package webhook provides mutating admission webhooks for JanusGuard.
//
// The GuardInjector webhook injects a guard-wait init container into pods
// that match a JanusGuard selector. This ensures the JanusGuard has registered
// fanotify marks before the main container starts, eliminating the race condition
// where file access could occur before protection is active.
package webhook

import (
	"context"
	"fmt"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/labels"
	"sigs.k8s.io/controller-runtime/pkg/client"

	commonwebhook "github.com/como-technologies/panoptes/operators/common/webhook"
	janusv2 "github.com/como-technologies/panoptes/operators/janus-operator/api/v2"
)

// GuardMatcher implements commonwebhook.ResourceMatcher for JanusGuard.
type GuardMatcher struct {
	Client client.Client
}

// FindMatchingResource finds a JanusGuard that matches the given pod.
func (m *GuardMatcher) FindMatchingResource(ctx context.Context, pod *corev1.Pod) (string, error) {
	// List all JanusGuards in the pod's namespace
	var guards janusv2.JanusGuardList
	if err := m.Client.List(ctx, &guards, client.InNamespace(pod.Namespace)); err != nil {
		return "", fmt.Errorf("failed to list JanusGuards: %w", err)
	}

	// Check each guard's selector against the pod
	for i := range guards.Items {
		guard := &guards.Items[i]

		// Skip paused guards
		if guard.Spec.Paused {
			continue
		}

		// Convert LabelSelector to labels.Selector
		selector, err := metav1.LabelSelectorAsSelector(&guard.Spec.Selector)
		if err != nil {
			continue
		}

		// Check if pod matches the guard's selector
		if selector.Matches(labels.Set(pod.Labels)) {
			return guard.Name, nil
		}
	}

	return "", nil
}

// NewGuardInjector creates a new GuardInjector webhook handler using the
// common GenericInjector with Janus-specific configuration.
//
// Configuration can be overridden via environment variables:
//   - GUARD_WAIT_IMAGE: Image for the guard-wait init container
//   - JANUSD_ADDRESS: Address of the janusd gRPC service
//   - GUARD_MAX_WAIT_SECS: Maximum time to wait for guard readiness
func NewGuardInjector(c client.Client) *commonwebhook.GenericInjector {
	config := commonwebhook.InjectorConfig{
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

	return commonwebhook.NewGenericInjector(c, config, &GuardMatcher{Client: c})
}
