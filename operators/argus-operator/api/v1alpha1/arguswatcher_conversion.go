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

	v1 "github.com/como-technologies/panoptes/operators/argus-operator/api/v1"
)

// ConvertTo converts this ArgusWatcher (v1alpha1) to the Hub version (v1).
func (src *ArgusWatcher) ConvertTo(dstRaw conversion.Hub) error {
	dst := dstRaw.(*v1.ArgusWatcher)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec conversion
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = v1.ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = false // New field in v1, default to false

	// Convert subjects
	dst.Spec.Subjects = make([]v1.ArgusWatcherSubject, len(src.Spec.Subjects))
	for i, subject := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = v1.ArgusWatcherSubject{
			Paths:      subject.Paths,
			Events:     convertEventsToV1(subject.Events),
			Ignore:     subject.Ignore,
			Recursive:  subject.Recursive,
			MaxDepth:   subject.MaxDepth,
			OnlyDir:    subject.OnlyDir,
			FollowMove: subject.FollowMove,
			Tags:       subject.Tags,
		}
	}

	// Status conversion
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.WatchedPods = src.Status.WatchedPods
	dst.Status.Conditions = src.Status.Conditions
	// New status fields in v1 will be zero-valued

	return nil
}

// ConvertFrom converts from the Hub version (v1) to this version (v1alpha1).
func (dst *ArgusWatcher) ConvertFrom(srcRaw conversion.Hub) error {
	src := srcRaw.(*v1.ArgusWatcher)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec conversion
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = string(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	// src.Spec.Paused is lost in v1alpha1 (field doesn't exist)

	// Convert subjects
	dst.Spec.Subjects = make([]ArgusWatcherSubject, len(src.Spec.Subjects))
	for i, subject := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = ArgusWatcherSubject{
			Paths:      subject.Paths,
			Events:     convertEventsFromV1(subject.Events),
			Ignore:     subject.Ignore,
			Recursive:  subject.Recursive,
			MaxDepth:   subject.MaxDepth,
			OnlyDir:    subject.OnlyDir,
			FollowMove: subject.FollowMove,
			Tags:       subject.Tags,
		}
	}

	// Status conversion
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.WatchedPods = src.Status.WatchedPods
	dst.Status.Conditions = src.Status.Conditions
	// New status fields from v1 are lost in v1alpha1

	return nil
}

// convertEventsToV1 converts v1alpha1 events to v1 events
func convertEventsToV1(events []ArgusEvent) []v1.ArgusEvent {
	result := make([]v1.ArgusEvent, len(events))
	for i, e := range events {
		result[i] = v1.ArgusEvent(e)
	}
	return result
}

// convertEventsFromV1 converts v1 events to v1alpha1 events
func convertEventsFromV1(events []v1.ArgusEvent) []ArgusEvent {
	result := make([]ArgusEvent, len(events))
	for i, e := range events {
		result[i] = ArgusEvent(e)
	}
	return result
}
