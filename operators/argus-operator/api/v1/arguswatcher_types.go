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
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// ArgusEvent represents an inotify event type for file integrity monitoring
// +kubebuilder:validation:Enum=access;attrib;closewrite;closenowrite;close;create;delete;deleteself;modify;moveself;movedfrom;movedto;move;open;all
type ArgusEvent string

const (
	EventAccess       ArgusEvent = "access"
	EventAttrib       ArgusEvent = "attrib"
	EventCloseWrite   ArgusEvent = "closewrite"
	EventCloseNoWrite ArgusEvent = "closenowrite"
	EventClose        ArgusEvent = "close"
	EventCreate       ArgusEvent = "create"
	EventDelete       ArgusEvent = "delete"
	EventDeleteSelf   ArgusEvent = "deleteself"
	EventModify       ArgusEvent = "modify"
	EventMoveSelf     ArgusEvent = "moveself"
	EventMovedFrom    ArgusEvent = "movedfrom"
	EventMovedTo      ArgusEvent = "movedto"
	EventMove         ArgusEvent = "move"
	EventOpen         ArgusEvent = "open"
	EventAll          ArgusEvent = "all"
)

// ContainerRuntime specifies the container runtime for PID detection
// +kubebuilder:validation:Enum=containerd;cri-o;auto
type ContainerRuntime string

const (
	RuntimeContainerd ContainerRuntime = "containerd"
	RuntimeCRIO       ContainerRuntime = "cri-o"
	RuntimeAuto       ContainerRuntime = "auto"
)

// ArgusWatcherSubject defines a watch target with paths and events
type ArgusWatcherSubject struct {
	// paths is the list of file or directory paths to monitor.
	// Paths are relative to the container's filesystem root.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=100
	Paths []string `json:"paths"`

	// events is the list of inotify events to watch for.
	// Use "all" to watch for all event types.
	// +kubebuilder:validation:MinItems=1
	Events []ArgusEvent `json:"events"`

	// ignore is the list of glob patterns to ignore.
	// Patterns follow standard glob syntax (e.g., "*.tmp", "**/node_modules/**").
	// +optional
	// +kubebuilder:validation:MaxItems=50
	Ignore []string `json:"ignore,omitempty"`

	// recursive enables recursive directory watching.
	// When true, all subdirectories are also monitored.
	// +optional
	Recursive bool `json:"recursive,omitempty"`

	// maxDepth limits the recursion depth when recursive is enabled.
	// A value of 0 means unlimited depth. Only effective when recursive is true.
	// +optional
	// +kubebuilder:validation:Minimum=0
	// +kubebuilder:validation:Maximum=100
	MaxDepth *int32 `json:"maxDepth,omitempty"`

	// onlyDir restricts watching to directories only.
	// File events in watched directories will still be reported.
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// followMove tracks moved files by inode.
	// When true, files are tracked across moves within the watched paths.
	// +optional
	FollowMove bool `json:"followMove,omitempty"`

	// tags are custom key-value metadata attached to events from this subject.
	// Useful for categorization, compliance tracking, or alert routing.
	// +optional
	// +kubebuilder:validation:MaxProperties=20
	Tags map[string]string `json:"tags,omitempty"`
}

// ArgusWatcherSpec defines the desired state of ArgusWatcher
type ArgusWatcherSpec struct {
	// selector is the label selector for pods to watch.
	// Only pods matching this selector will be monitored.
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of watch targets defining what to monitor.
	// Each subject specifies paths and events to watch.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=20
	Subjects []ArgusWatcherSubject `json:"subjects"`

	// containerRuntime specifies which container runtime to use for PID detection.
	// Use "auto" to automatically detect the runtime.
	// +kubebuilder:validation:Enum=containerd;cri-o;auto
	// +kubebuilder:default=auto
	// +optional
	ContainerRuntime ContainerRuntime `json:"containerRuntime,omitempty"`

	// logFormat is the custom log format template for events.
	// Supports Go template syntax with access to event fields.
	// +optional
	// +kubebuilder:validation:MaxLength=1024
	LogFormat string `json:"logFormat,omitempty"`

	// paused indicates whether watching is paused for this resource.
	// When true, no new watches will be created but existing ones are maintained.
	// +optional
	Paused bool `json:"paused,omitempty"`
}

// WatchedPodStatus represents the watch status for a single pod
type WatchedPodStatus struct {
	// name is the name of the pod being watched
	Name string `json:"name"`

	// namespace is the namespace of the pod
	Namespace string `json:"namespace"`

	// nodeName is the node where the pod is running
	NodeName string `json:"nodeName"`

	// watchDescriptors is the number of inotify watch descriptors for this pod
	WatchDescriptors int32 `json:"watchDescriptors"`

	// lastEventTime is when the last event was received from this pod
	// +optional
	LastEventTime *metav1.Time `json:"lastEventTime,omitempty"`
}

// ArgusWatcherStatus defines the observed state of ArgusWatcher
type ArgusWatcherStatus struct {
	// observedGeneration is the most recent generation observed by the controller.
	// Used to determine if the status is up-to-date with the spec.
	// +optional
	ObservedGeneration int64 `json:"observedGeneration,omitempty"`

	// observablePods is the number of pods matching the selector.
	// This is the total number of pods that could potentially be watched.
	// +optional
	ObservablePods int32 `json:"observablePods"`

	// watchedPods is the number of pods currently being watched.
	// This should eventually equal observablePods when fully reconciled.
	// +optional
	WatchedPods int32 `json:"watchedPods"`

	// totalWatchDescriptors is the total number of inotify watch descriptors in use.
	// High values may indicate the need to increase inotify limits.
	// +optional
	TotalWatchDescriptors int32 `json:"totalWatchDescriptors,omitempty"`

	// eventsDetected is the total count of file events detected since creation.
	// +optional
	EventsDetected int64 `json:"eventsDetected,omitempty"`

	// lastReconcileTime is when the controller last reconciled this resource.
	// +optional
	LastReconcileTime *metav1.Time `json:"lastReconcileTime,omitempty"`

	// podStatuses contains detailed status for each watched pod.
	// +optional
	// +listType=map
	// +listMapKey=name
	PodStatuses []WatchedPodStatus `json:"podStatuses,omitempty"`

	// conditions represent the current state of the ArgusWatcher resource.
	// Standard condition types: Available, Progressing, Degraded
	// +listType=map
	// +listMapKey=type
	// +optional
	Conditions []metav1.Condition `json:"conditions,omitempty"`
}

// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
// +kubebuilder:storageversion
// +kubebuilder:resource:shortName=aw,categories=all;argus;security
// +kubebuilder:printcolumn:name="Observable",type=integer,JSONPath=`.status.observablePods`,description="Number of pods matching selector"
// +kubebuilder:printcolumn:name="Watched",type=integer,JSONPath=`.status.watchedPods`,description="Number of pods being watched"
// +kubebuilder:printcolumn:name="Events",type=integer,JSONPath=`.status.eventsDetected`,description="Total events detected"
// +kubebuilder:printcolumn:name="Paused",type=boolean,JSONPath=`.spec.paused`,description="Whether watching is paused"
// +kubebuilder:printcolumn:name="Age",type=date,JSONPath=`.metadata.creationTimestamp`

// ArgusWatcher is the Schema for the arguswatchers API.
// It defines file integrity monitoring rules for pods matching a selector.
type ArgusWatcher struct {
	metav1.TypeMeta   `json:",inline"`
	metav1.ObjectMeta `json:"metadata,omitempty"`

	// spec defines the desired state of ArgusWatcher
	// +required
	Spec ArgusWatcherSpec `json:"spec"`

	// status defines the observed state of ArgusWatcher
	// +optional
	Status ArgusWatcherStatus `json:"status,omitempty"`
}

// +kubebuilder:object:root=true

// ArgusWatcherList contains a list of ArgusWatcher
type ArgusWatcherList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitempty"`
	Items           []ArgusWatcher `json:"items"`
}

func init() {
	SchemeBuilder.Register(&ArgusWatcher{}, &ArgusWatcherList{})
}
