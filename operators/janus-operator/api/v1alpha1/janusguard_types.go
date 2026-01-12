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

// JanusEvent represents a fanotify event type for file access auditing
// +kubebuilder:validation:Enum=access;open;execute;all
type JanusEvent string

const (
	EventAccess  JanusEvent = "access"
	EventOpen    JanusEvent = "open"
	EventExecute JanusEvent = "execute"
	EventAll     JanusEvent = "all"
)

// JanusResponse represents the action to take when an event is matched
// +kubebuilder:validation:Enum=allow;deny;audit
type JanusResponse string

const (
	ResponseAllow JanusResponse = "allow"
	ResponseDeny  JanusResponse = "deny"
	ResponseAudit JanusResponse = "audit"
)

// JanusGuardSubject defines an access control target with allow/deny rules
type JanusGuardSubject struct {
	// allow is the list of paths to allow access to
	// +optional
	Allow []string `json:"allow,omitempty"`

	// deny is the list of paths to deny access to
	// +optional
	Deny []string `json:"deny,omitempty"`

	// events is the list of fanotify events to audit
	// +kubebuilder:validation:MinItems=1
	Events []JanusEvent `json:"events"`

	// onlyDir restricts auditing to directories only
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// autoAllowOwner automatically allows access for the process owner
	// +optional
	AutoAllowOwner bool `json:"autoAllowOwner,omitempty"`

	// audit enables kernel audit log integration
	// +optional
	Audit bool `json:"audit,omitempty"`

	// defaultResponse is the default action when no rule matches
	// +kubebuilder:validation:Enum=allow;deny;audit
	// +kubebuilder:default=audit
	// +optional
	DefaultResponse JanusResponse `json:"defaultResponse,omitempty"`

	// tags are custom key-value metadata for the guard
	// +optional
	Tags map[string]string `json:"tags,omitempty"`
}

// JanusGuardSpec defines the desired state of JanusGuard
type JanusGuardSpec struct {
	// selector is the label selector for pods to guard
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of access control targets
	// +kubebuilder:validation:MinItems=1
	Subjects []JanusGuardSubject `json:"subjects"`

	// containerRuntime specifies which container runtime to use for PID detection
	// +kubebuilder:validation:Enum=containerd;cri-o;auto
	// +kubebuilder:default=auto
	// +optional
	ContainerRuntime string `json:"containerRuntime,omitempty"`

	// logFormat is the custom log format template
	// +optional
	LogFormat string `json:"logFormat,omitempty"`
}

// JanusGuardStatus defines the observed state of JanusGuard
type JanusGuardStatus struct {
	// observedGeneration is the most recent generation observed
	// +optional
	ObservedGeneration int64 `json:"observedGeneration,omitempty"`

	// observablePods is the number of pods matching the selector
	// +optional
	ObservablePods int32 `json:"observablePods"`

	// guardedPods is the number of pods currently being guarded
	// +optional
	GuardedPods int32 `json:"guardedPods"`

	// deniedEvents is the total count of denied access attempts
	// +optional
	DeniedEvents int64 `json:"deniedEvents,omitempty"`

	// conditions represent the current state of the JanusGuard resource
	// +listType=map
	// +listMapKey=type
	// +optional
	Conditions []metav1.Condition `json:"conditions,omitempty"`
}

// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
// +kubebuilder:resource:shortName=jg,categories=all;janus;security
// +kubebuilder:printcolumn:name="Observable",type=integer,JSONPath=`.status.observablePods`,description="Number of pods matching selector"
// +kubebuilder:printcolumn:name="Guarded",type=integer,JSONPath=`.status.guardedPods`,description="Number of pods being guarded"
// +kubebuilder:printcolumn:name="Denied",type=integer,JSONPath=`.status.deniedEvents`,description="Total denied access attempts"
// +kubebuilder:printcolumn:name="Age",type=date,JSONPath=`.metadata.creationTimestamp`

// JanusGuard is the Schema for the janusguards API
// It defines file access auditing and control rules for pods matching a selector
type JanusGuard struct {
	metav1.TypeMeta   `json:",inline"`
	metav1.ObjectMeta `json:"metadata,omitempty"`

	// spec defines the desired state of JanusGuard
	// +required
	Spec JanusGuardSpec `json:"spec"`

	// status defines the observed state of JanusGuard
	// +optional
	Status JanusGuardStatus `json:"status,omitempty"`
}

// +kubebuilder:object:root=true

// JanusGuardList contains a list of JanusGuard
type JanusGuardList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitempty"`
	Items           []JanusGuard `json:"items"`
}

func init() {
	SchemeBuilder.Register(&JanusGuard{}, &JanusGuardList{})
}
