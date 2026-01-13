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

package webhook

import (
	"context"
	"encoding/json"
	"net/http"

	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/resource"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/controller-runtime/pkg/webhook/admission"
)

// ResourceMatcher finds a matching custom resource for a pod.
// This interface is implemented by daemon-specific code (ArgusWatcher, JanusGuard).
type ResourceMatcher interface {
	// FindMatchingResource returns the name of the matching resource if found.
	// Returns empty string if no match, or error on failure.
	FindMatchingResource(ctx context.Context, pod *corev1.Pod) (resourceName string, err error)
}

// GenericInjector is a mutating webhook that injects init containers into pods
// that match a daemon-specific resource selector.
type GenericInjector struct {
	Client  client.Client
	Decoder admission.Decoder
	Config  InjectorConfig
	Matcher ResourceMatcher
}

// NewGenericInjector creates a new GenericInjector webhook handler.
func NewGenericInjector(c client.Client, config InjectorConfig, matcher ResourceMatcher) *GenericInjector {
	return &GenericInjector{
		Client:  c,
		Config:  config,
		Matcher: matcher,
	}
}

// Handle implements admission.Handler.
func (g *GenericInjector) Handle(ctx context.Context, req admission.Request) admission.Response {
	logger := log.FromContext(ctx).WithValues(
		"webhook", g.Config.WebhookName,
		"namespace", req.Namespace,
		"name", req.Name,
	)

	pod := &corev1.Pod{}
	if err := g.Decoder.Decode(req, pod); err != nil {
		logger.Error(err, "Failed to decode pod")
		return admission.Errored(http.StatusBadRequest, err)
	}

	// Check if injection is explicitly disabled
	if val, ok := pod.Annotations[g.Config.AnnotationInject()]; ok && val == "false" {
		logger.V(1).Info("Injection disabled via annotation")
		return admission.Allowed("injection disabled")
	}

	// Check if already injected
	if g.hasInitContainer(pod) {
		logger.V(1).Info("Pod already has init container")
		return admission.Allowed("already injected")
	}

	// Find matching resource for this pod
	resourceName, err := g.Matcher.FindMatchingResource(ctx, pod)
	if err != nil {
		logger.Error(err, "Failed to find matching resource")
		return admission.Errored(http.StatusInternalServerError, err)
	}

	if resourceName == "" {
		logger.V(1).Info("No matching resource found for pod", "resourceType", g.Config.ResourceTypeName)
		return admission.Allowed("no matching resource")
	}

	logger.Info("Injecting init container",
		"resource", resourceName,
		"resourceType", g.Config.ResourceTypeName,
	)

	// Inject the init container
	g.injectInitContainer(pod, resourceName)

	// Add annotation and label
	if pod.Annotations == nil {
		pod.Annotations = make(map[string]string)
	}
	pod.Annotations[g.Config.AnnotationResourceName()] = resourceName

	if pod.Labels == nil {
		pod.Labels = make(map[string]string)
	}
	pod.Labels[g.Config.LabelInjected()] = "true"

	// Create patch
	marshaledPod, err := json.Marshal(pod)
	if err != nil {
		logger.Error(err, "Failed to marshal pod")
		return admission.Errored(http.StatusInternalServerError, err)
	}

	return admission.PatchResponseFromRaw(req.Object.Raw, marshaledPod)
}

// hasInitContainer checks if the pod already has the init container.
func (g *GenericInjector) hasInitContainer(pod *corev1.Pod) bool {
	for _, c := range pod.Spec.InitContainers {
		if c.Name == g.Config.InitContainerName {
			return true
		}
	}
	return false
}

// injectInitContainer adds the init container to the pod.
func (g *GenericInjector) injectInitContainer(pod *corev1.Pod, resourceName string) {
	initContainer := corev1.Container{
		Name:            g.Config.InitContainerName,
		Image:           g.Config.GetImage(),
		ImagePullPolicy: corev1.PullIfNotPresent,
		Env: []corev1.EnvVar{
			{
				Name:  g.Config.ResourceNameEnvVar,
				Value: resourceName,
			},
			{
				Name: "NAMESPACE",
				ValueFrom: &corev1.EnvVarSource{
					FieldRef: &corev1.ObjectFieldSelector{
						FieldPath: "metadata.namespace",
					},
				},
			},
			{
				Name: "POD_NAME",
				ValueFrom: &corev1.EnvVarSource{
					FieldRef: &corev1.ObjectFieldSelector{
						FieldPath: "metadata.name",
					},
				},
			},
			{
				Name:  g.Config.AddressEnvVar,
				Value: g.Config.GetAddress(),
			},
			{
				Name:  "MAX_WAIT_SECS",
				Value: g.Config.GetMaxWaitSecs(),
			},
		},
		// Resource limits for init container
		Resources: corev1.ResourceRequirements{
			Limits: corev1.ResourceList{
				corev1.ResourceCPU:    resource.MustParse("100m"),
				corev1.ResourceMemory: resource.MustParse("32Mi"),
			},
			Requests: corev1.ResourceList{
				corev1.ResourceCPU:    resource.MustParse("10m"),
				corev1.ResourceMemory: resource.MustParse("8Mi"),
			},
		},
	}

	// Prepend to init containers (run first)
	pod.Spec.InitContainers = append([]corev1.Container{initContainer}, pod.Spec.InitContainers...)
}
