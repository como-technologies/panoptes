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

// Package webhook provides mutating admission webhooks for ArgusWatcher.
//
// The WatcherInjector webhook injects a watcher-wait init container into pods
// that match an ArgusWatcher selector. This ensures the ArgusWatcher has registered
// inotify watches before the main container starts, eliminating the race condition
// where file modification could occur before protection is active.
package webhook

import (
	"context"
	"fmt"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/labels"
	"sigs.k8s.io/controller-runtime/pkg/client"

	argusv2 "github.com/como-technologies/panoptes/operators/argus-operator/api/v2"
	commonwebhook "github.com/como-technologies/panoptes/operators/common/webhook"
)

// WatcherMatcher implements commonwebhook.ResourceMatcher for ArgusWatcher.
type WatcherMatcher struct {
	Client client.Client
}

// FindMatchingResource finds an ArgusWatcher that matches the given pod.
func (m *WatcherMatcher) FindMatchingResource(ctx context.Context, pod *corev1.Pod) (string, error) {
	// List all ArgusWatchers in the pod's namespace
	var watchers argusv2.ArgusWatcherList
	if err := m.Client.List(ctx, &watchers, client.InNamespace(pod.Namespace)); err != nil {
		return "", fmt.Errorf("failed to list ArgusWatchers: %w", err)
	}

	// Check each watcher's selector against the pod
	for i := range watchers.Items {
		watcher := &watchers.Items[i]

		// Skip paused watchers
		if watcher.Spec.Paused {
			continue
		}

		// Convert LabelSelector to labels.Selector
		selector, err := metav1.LabelSelectorAsSelector(&watcher.Spec.Selector)
		if err != nil {
			continue
		}

		// Check if pod matches the watcher's selector
		if selector.Matches(labels.Set(pod.Labels)) {
			return watcher.Name, nil
		}
	}

	return "", nil
}

// NewWatcherInjector creates a new WatcherInjector webhook handler using the
// common GenericInjector with Argus-specific configuration.
//
// Configuration can be overridden via environment variables:
//   - WATCHER_WAIT_IMAGE: Image for the watcher-wait init container
//   - ARGUSD_ADDRESS: Address of the argusd gRPC service
//   - WATCHER_MAX_WAIT_SECS: Maximum time to wait for watcher readiness
func NewWatcherInjector(c client.Client) *commonwebhook.GenericInjector {
	config := commonwebhook.InjectorConfig{
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

	return commonwebhook.NewGenericInjector(c, config, &WatcherMatcher{Client: c})
}
