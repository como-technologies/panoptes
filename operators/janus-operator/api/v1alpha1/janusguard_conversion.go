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

package v1alpha1

import (
	"sigs.k8s.io/controller-runtime/pkg/conversion"

	v1 "github.com/como-technologies/panoptes/operators/janus-operator/api/v1"
)

// ConvertTo converts this JanusGuard (v1alpha1) to the Hub version (v1).
func (src *JanusGuard) ConvertTo(dstRaw conversion.Hub) error {
	dst := dstRaw.(*v1.JanusGuard)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec conversion
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = v1.ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = false    // New field in v1, default to false
	dst.Spec.Enforcing = true  // New field in v1, default to true

	// Convert subjects
	dst.Spec.Subjects = make([]v1.JanusGuardSubject, len(src.Spec.Subjects))
	for i, subject := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = v1.JanusGuardSubject{
			Allow:           subject.Allow,
			Deny:            subject.Deny,
			Events:          convertEventsToV1(subject.Events),
			OnlyDir:         subject.OnlyDir,
			AutoAllowOwner:  subject.AutoAllowOwner,
			Audit:           subject.Audit,
			DefaultResponse: v1.JanusResponse(subject.DefaultResponse),
			Tags:            subject.Tags,
		}
	}

	// Status conversion
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.GuardedPods = src.Status.GuardedPods
	dst.Status.TotalDeniedEvents = src.Status.DeniedEvents
	dst.Status.Conditions = src.Status.Conditions
	// New status fields in v1 will be zero-valued

	return nil
}

// ConvertFrom converts from the Hub version (v1) to this version (v1alpha1).
func (dst *JanusGuard) ConvertFrom(srcRaw conversion.Hub) error {
	src := srcRaw.(*v1.JanusGuard)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec conversion
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = string(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	// src.Spec.Paused and src.Spec.Enforcing are lost in v1alpha1

	// Convert subjects
	dst.Spec.Subjects = make([]JanusGuardSubject, len(src.Spec.Subjects))
	for i, subject := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = JanusGuardSubject{
			Allow:           subject.Allow,
			Deny:            subject.Deny,
			Events:          convertEventsFromV1(subject.Events),
			OnlyDir:         subject.OnlyDir,
			AutoAllowOwner:  subject.AutoAllowOwner,
			Audit:           subject.Audit,
			DefaultResponse: JanusResponse(subject.DefaultResponse),
			Tags:            subject.Tags,
		}
	}

	// Status conversion
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.GuardedPods = src.Status.GuardedPods
	dst.Status.DeniedEvents = src.Status.TotalDeniedEvents
	dst.Status.Conditions = src.Status.Conditions
	// New status fields from v1 are lost in v1alpha1

	return nil
}

// convertEventsToV1 converts v1alpha1 events to v1 events
func convertEventsToV1(events []JanusEvent) []v1.JanusEvent {
	result := make([]v1.JanusEvent, len(events))
	for i, e := range events {
		result[i] = v1.JanusEvent(e)
	}
	return result
}

// convertEventsFromV1 converts v1 events to v1alpha1 events
func convertEventsFromV1(events []v1.JanusEvent) []JanusEvent {
	result := make([]JanusEvent, len(events))
	for i, e := range events {
		result[i] = JanusEvent(e)
	}
	return result
}
