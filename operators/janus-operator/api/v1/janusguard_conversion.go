/*
Copyright 2026.

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

package v1

import (
	"sigs.k8s.io/controller-runtime/pkg/conversion"

	v2 "github.com/como-technologies/panoptes/operators/janus-operator/api/v2"
)

// ConvertTo converts this JanusGuard (v1) to the Hub version (v2).
func (src *JanusGuard) ConvertTo(dstRaw conversion.Hub) error {
	dst := dstRaw.(*v2.JanusGuard)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = v2.ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = src.Spec.Paused
	dst.Spec.Enforcing = src.Spec.Enforcing

	// Subjects
	dst.Spec.Subjects = make([]v2.JanusGuardSubject, len(src.Spec.Subjects))
	for i, s := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = v2.JanusGuardSubject{
			Allow:           s.Allow,
			Deny:            s.Deny,
			Events:          convertJanusEventsToV2(s.Events),
			OnlyDir:         s.OnlyDir,
			AutoAllowOwner:  s.AutoAllowOwner,
			Audit:           s.Audit,
			DefaultResponse: v2.JanusResponse(s.DefaultResponse),
			Tags:            s.Tags,
		}
	}

	// Status
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.GuardedPods = src.Status.GuardedPods
	dst.Status.TotalDeniedEvents = src.Status.TotalDeniedEvents
	dst.Status.TotalAllowedEvents = src.Status.TotalAllowedEvents
	dst.Status.TotalAuditedEvents = src.Status.TotalAuditedEvents
	dst.Status.LastReconcileTime = src.Status.LastReconcileTime
	dst.Status.Conditions = src.Status.Conditions

	// PodStatuses
	dst.Status.PodStatuses = make([]v2.GuardedPodStatus, len(src.Status.PodStatuses))
	for i, ps := range src.Status.PodStatuses {
		dst.Status.PodStatuses[i] = v2.GuardedPodStatus{
			Name:           ps.Name,
			Namespace:      ps.Namespace,
			NodeName:       ps.NodeName,
			DeniedCount:    ps.DeniedCount,
			AllowedCount:   ps.AllowedCount,
			LastDenialTime: ps.LastDenialTime,
		}
	}

	return nil
}

// ConvertFrom converts the Hub version (v2) to this JanusGuard (v1).
func (dst *JanusGuard) ConvertFrom(srcRaw conversion.Hub) error {
	src := srcRaw.(*v2.JanusGuard)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = src.Spec.Paused
	dst.Spec.Enforcing = src.Spec.Enforcing

	// Subjects
	dst.Spec.Subjects = make([]JanusGuardSubject, len(src.Spec.Subjects))
	for i, s := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = JanusGuardSubject{
			Allow:           s.Allow,
			Deny:            s.Deny,
			Events:          convertJanusEventsFromV2(s.Events),
			OnlyDir:         s.OnlyDir,
			AutoAllowOwner:  s.AutoAllowOwner,
			Audit:           s.Audit,
			DefaultResponse: JanusResponse(s.DefaultResponse),
			Tags:            s.Tags,
		}
	}

	// Status
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.GuardedPods = src.Status.GuardedPods
	dst.Status.TotalDeniedEvents = src.Status.TotalDeniedEvents
	dst.Status.TotalAllowedEvents = src.Status.TotalAllowedEvents
	dst.Status.TotalAuditedEvents = src.Status.TotalAuditedEvents
	dst.Status.LastReconcileTime = src.Status.LastReconcileTime
	dst.Status.Conditions = src.Status.Conditions

	// PodStatuses
	dst.Status.PodStatuses = make([]GuardedPodStatus, len(src.Status.PodStatuses))
	for i, ps := range src.Status.PodStatuses {
		dst.Status.PodStatuses[i] = GuardedPodStatus{
			Name:           ps.Name,
			Namespace:      ps.Namespace,
			NodeName:       ps.NodeName,
			DeniedCount:    ps.DeniedCount,
			AllowedCount:   ps.AllowedCount,
			LastDenialTime: ps.LastDenialTime,
		}
	}

	return nil
}

func convertJanusEventsToV2(events []JanusEvent) []v2.JanusEvent {
	result := make([]v2.JanusEvent, len(events))
	for i, e := range events {
		result[i] = v2.JanusEvent(e)
	}
	return result
}

func convertJanusEventsFromV2(events []v2.JanusEvent) []JanusEvent {
	result := make([]JanusEvent, len(events))
	for i, e := range events {
		result[i] = JanusEvent(e)
	}
	return result
}
