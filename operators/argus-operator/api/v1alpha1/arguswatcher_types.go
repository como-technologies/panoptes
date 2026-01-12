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
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// ArgusEvent represents an inotify event type
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

// ArgusWatcherSubject defines a watch target with paths and events
type ArgusWatcherSubject struct {
	// paths is the list of file or directory paths to monitor
	// +kubebuilder:validation:MinItems=1
	Paths []string `json:"paths"`

	// events is the list of inotify events to watch for
	// +kubebuilder:validation:MinItems=1
	Events []ArgusEvent `json:"events"`

	// ignore is the list of path patterns to ignore
	// +optional
	Ignore []string `json:"ignore,omitempty"`

	// recursive enables recursive directory watching
	// +optional
	Recursive bool `json:"recursive,omitempty"`

	// maxDepth limits the recursion depth when recursive is enabled
	// +optional
	// +kubebuilder:validation:Minimum=0
	MaxDepth *int32 `json:"maxDepth,omitempty"`

	// onlyDir restricts watching to directories only
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// followMove tracks moved files by inode
	// +optional
	FollowMove bool `json:"followMove,omitempty"`

	// tags are custom key-value metadata for the watch
	// +optional
	Tags map[string]string `json:"tags,omitempty"`
}

// ArgusWatcherSpec defines the desired state of ArgusWatcher
type ArgusWatcherSpec struct {
	// selector is the label selector for pods to watch
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of watch targets
	// +kubebuilder:validation:MinItems=1
	Subjects []ArgusWatcherSubject `json:"subjects"`

	// containerRuntime specifies which container runtime to use for PID detection
	// +kubebuilder:validation:Enum=containerd;cri-o;auto
	// +kubebuilder:default=auto
	// +optional
	ContainerRuntime string `json:"containerRuntime,omitempty"`

	// logFormat is the custom log format template
	// +optional
	LogFormat string `json:"logFormat,omitempty"`
}

// ArgusWatcherStatus defines the observed state of ArgusWatcher
type ArgusWatcherStatus struct {
	// observedGeneration is the most recent generation observed
	// +optional
	ObservedGeneration int64 `json:"observedGeneration,omitempty"`

	// observablePods is the number of pods matching the selector
	// +optional
	ObservablePods int32 `json:"observablePods"`

	// watchedPods is the number of pods currently being watched
	// +optional
	WatchedPods int32 `json:"watchedPods"`

	// conditions represent the current state of the ArgusWatcher resource
	// +listType=map
	// +listMapKey=type
	// +optional
	Conditions []metav1.Condition `json:"conditions,omitempty"`
}

// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
// +kubebuilder:resource:shortName=aw,categories=all;argus;security
// +kubebuilder:printcolumn:name="Observable",type=integer,JSONPath=`.status.observablePods`,description="Number of pods matching selector"
// +kubebuilder:printcolumn:name="Watched",type=integer,JSONPath=`.status.watchedPods`,description="Number of pods being watched"
// +kubebuilder:printcolumn:name="Age",type=date,JSONPath=`.metadata.creationTimestamp`

// ArgusWatcher is the Schema for the arguswatchers API
// It defines file integrity monitoring rules for pods matching a selector
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
