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

	v2 "github.com/como-technologies/panoptes/operators/argus-operator/api/v2"
)

// ConvertTo converts this ArgusWatcher (v1) to the Hub version (v2).
func (src *ArgusWatcher) ConvertTo(dstRaw conversion.Hub) error {
	dst := dstRaw.(*v2.ArgusWatcher)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = v2.ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = src.Spec.Paused

	// Subjects
	dst.Spec.Subjects = make([]v2.ArgusWatcherSubject, len(src.Spec.Subjects))
	for i, s := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = v2.ArgusWatcherSubject{
			Paths:      s.Paths,
			Events:     convertArgusEventsToV2(s.Events),
			Ignore:     s.Ignore,
			Recursive:  s.Recursive,
			MaxDepth:   s.MaxDepth,
			OnlyDir:    s.OnlyDir,
			FollowMove: s.FollowMove,
			Tags:       s.Tags,
		}
	}

	// Status
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.WatchedPods = src.Status.WatchedPods
	dst.Status.TotalWatchDescriptors = src.Status.TotalWatchDescriptors
	dst.Status.EventsDetected = src.Status.EventsDetected
	dst.Status.LastReconcileTime = src.Status.LastReconcileTime
	dst.Status.Conditions = src.Status.Conditions

	// PodStatuses
	dst.Status.PodStatuses = make([]v2.WatchedPodStatus, len(src.Status.PodStatuses))
	for i, ps := range src.Status.PodStatuses {
		dst.Status.PodStatuses[i] = v2.WatchedPodStatus{
			Name:             ps.Name,
			Namespace:        ps.Namespace,
			NodeName:         ps.NodeName,
			WatchDescriptors: ps.WatchDescriptors,
			LastEventTime:    ps.LastEventTime,
		}
	}

	return nil
}

// ConvertFrom converts the Hub version (v2) to this ArgusWatcher (v1).
func (dst *ArgusWatcher) ConvertFrom(srcRaw conversion.Hub) error {
	src := srcRaw.(*v2.ArgusWatcher)

	// ObjectMeta
	dst.ObjectMeta = src.ObjectMeta

	// Spec
	dst.Spec.Selector = src.Spec.Selector
	dst.Spec.ContainerRuntime = ContainerRuntime(src.Spec.ContainerRuntime)
	dst.Spec.LogFormat = src.Spec.LogFormat
	dst.Spec.Paused = src.Spec.Paused

	// Subjects
	dst.Spec.Subjects = make([]ArgusWatcherSubject, len(src.Spec.Subjects))
	for i, s := range src.Spec.Subjects {
		dst.Spec.Subjects[i] = ArgusWatcherSubject{
			Paths:      s.Paths,
			Events:     convertArgusEventsFromV2(s.Events),
			Ignore:     s.Ignore,
			Recursive:  s.Recursive,
			MaxDepth:   s.MaxDepth,
			OnlyDir:    s.OnlyDir,
			FollowMove: s.FollowMove,
			Tags:       s.Tags,
		}
	}

	// Status
	dst.Status.ObservedGeneration = src.Status.ObservedGeneration
	dst.Status.ObservablePods = src.Status.ObservablePods
	dst.Status.WatchedPods = src.Status.WatchedPods
	dst.Status.TotalWatchDescriptors = src.Status.TotalWatchDescriptors
	dst.Status.EventsDetected = src.Status.EventsDetected
	dst.Status.LastReconcileTime = src.Status.LastReconcileTime
	dst.Status.Conditions = src.Status.Conditions

	// PodStatuses
	dst.Status.PodStatuses = make([]WatchedPodStatus, len(src.Status.PodStatuses))
	for i, ps := range src.Status.PodStatuses {
		dst.Status.PodStatuses[i] = WatchedPodStatus{
			Name:             ps.Name,
			Namespace:        ps.Namespace,
			NodeName:         ps.NodeName,
			WatchDescriptors: ps.WatchDescriptors,
			LastEventTime:    ps.LastEventTime,
		}
	}

	return nil
}

func convertArgusEventsToV2(events []ArgusEvent) []v2.ArgusEvent {
	result := make([]v2.ArgusEvent, len(events))
	for i, e := range events {
		result[i] = v2.ArgusEvent(e)
	}
	return result
}

func convertArgusEventsFromV2(events []v2.ArgusEvent) []ArgusEvent {
	result := make([]ArgusEvent, len(events))
	for i, e := range events {
		result[i] = ArgusEvent(e)
	}
	return result
}
