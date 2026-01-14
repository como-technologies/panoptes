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

package v2

import (
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// JanusEvent represents a fanotify event type for file access auditing
// +kubebuilder:validation:Enum=access;open;execute;close;all
type JanusEvent string

const (
	EventAccess  JanusEvent = "access"
	EventOpen    JanusEvent = "open"
	EventExecute JanusEvent = "execute"
	EventClose   JanusEvent = "close"
	EventAll     JanusEvent = "all"
)

// JanusResponse represents the action to take when an access event is detected
// +kubebuilder:validation:Enum=allow;deny;audit
type JanusResponse string

const (
	ResponseAllow JanusResponse = "allow"
	ResponseDeny  JanusResponse = "deny"
	ResponseAudit JanusResponse = "audit"
)

// ContainerRuntime specifies the container runtime for PID detection
// +kubebuilder:validation:Enum=containerd;cri-o;auto
type ContainerRuntime string

const (
	RuntimeContainerd ContainerRuntime = "containerd"
	RuntimeCRIO       ContainerRuntime = "cri-o"
	RuntimeAuto       ContainerRuntime = "auto"
)

// JanusGuardSubject defines file access control rules
type JanusGuardSubject struct {
	// allow is the list of paths to explicitly allow access to.
	// Paths support glob patterns (e.g., "/app/**", "/etc/*.conf").
	// +optional
	// +kubebuilder:validation:MaxItems=100
	Allow []string `json:"allow,omitempty"`

	// deny is the list of paths to explicitly deny access to.
	// Deny rules take precedence over allow rules.
	// +optional
	// +kubebuilder:validation:MaxItems=100
	Deny []string `json:"deny,omitempty"`

	// events is the list of fanotify event types to monitor.
	// Use "all" to monitor all event types.
	// +kubebuilder:validation:MinItems=1
	Events []JanusEvent `json:"events"`

	// onlyDir restricts monitoring to directories only.
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// autoAllowOwner automatically allows access for the file owner.
	// Useful for applications that need to access their own files.
	// +optional
	AutoAllowOwner bool `json:"autoAllowOwner,omitempty"`

	// audit enables kernel audit log integration.
	// When true, events are also written to the kernel audit log.
	// +optional
	Audit bool `json:"audit,omitempty"`

	// defaultResponse is the action when no allow/deny rule matches.
	// +kubebuilder:validation:Enum=allow;deny;audit
	// +kubebuilder:default=audit
	// +optional
	DefaultResponse JanusResponse `json:"defaultResponse,omitempty"`

	// tags are custom key-value metadata attached to events from this subject.
	// Useful for categorization, compliance tracking, or alert routing.
	// +optional
	// +kubebuilder:validation:MaxProperties=20
	Tags map[string]string `json:"tags,omitempty"`
}

// JanusGuardSpec defines the desired state of JanusGuard
type JanusGuardSpec struct {
	// selector is the label selector for pods to guard.
	// Only pods matching this selector will have access control applied.
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of access control rules to apply.
	// Each subject defines allow/deny rules for file access.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=20
	Subjects []JanusGuardSubject `json:"subjects"`

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

	// paused indicates whether guarding is paused for this resource.
	// When true, no access control is enforced but existing guards are maintained.
	// +optional
	Paused bool `json:"paused,omitempty"`

	// enforcing indicates whether access denials are enforced.
	// When false, denials are logged but access is allowed (dry-run mode).
	// +kubebuilder:default=true
	// +optional
	Enforcing bool `json:"enforcing,omitempty"`
}

// GuardedPodStatus represents the guard status for a single pod
type GuardedPodStatus struct {
	// name is the name of the pod being guarded
	Name string `json:"name"`

	// namespace is the namespace of the pod
	Namespace string `json:"namespace"`

	// nodeName is the node where the pod is running
	NodeName string `json:"nodeName"`

	// deniedCount is the number of denied access attempts for this pod
	DeniedCount int64 `json:"deniedCount"`

	// allowedCount is the number of allowed access events for this pod
	AllowedCount int64 `json:"allowedCount"`

	// marksRegistered indicates fanotify marks are active for this pod.
	// +optional
	MarksRegistered bool `json:"marksRegistered,omitempty"`

	// readyAt is when guards became ready for this pod.
	// +optional
	ReadyAt *metav1.Time `json:"readyAt,omitempty"`

	// mountCount is the number of container mounts with active marks.
	// +optional
	MountCount int32 `json:"mountCount,omitempty"`

	// lastDenialTime is when the last access denial occurred
	// +optional
	LastDenialTime *metav1.Time `json:"lastDenialTime,omitempty"`
}

// JanusGuardStatus defines the observed state of JanusGuard
type JanusGuardStatus struct {
	// observedGeneration is the most recent generation observed by the controller.
	// Used to determine if the status is up-to-date with the spec.
	// +optional
	ObservedGeneration int64 `json:"observedGeneration,omitempty"`

	// observablePods is the number of pods matching the selector.
	// This is the total number of pods that could potentially be guarded.
	// +optional
	ObservablePods int32 `json:"observablePods"`

	// guardedPods is the number of pods currently being guarded.
	// This should eventually equal observablePods when fully reconciled.
	// +optional
	GuardedPods int32 `json:"guardedPods"`

	// marksRegistered indicates all fanotify marks are registered.
	// +optional
	MarksRegistered bool `json:"marksRegistered,omitempty"`

	// readyAt is when the guard became fully ready (all marks registered).
	// +optional
	ReadyAt *metav1.Time `json:"readyAt,omitempty"`

	// totalMountCount is the total number of container mounts being guarded.
	// +optional
	TotalMountCount int32 `json:"totalMountCount,omitempty"`

	// totalDeniedEvents is the total count of denied access attempts since creation.
	// +optional
	TotalDeniedEvents int64 `json:"totalDeniedEvents,omitempty"`

	// totalAllowedEvents is the total count of allowed access events since creation.
	// +optional
	TotalAllowedEvents int64 `json:"totalAllowedEvents,omitempty"`

	// totalAuditedEvents is the total count of audit-only events since creation.
	// +optional
	TotalAuditedEvents int64 `json:"totalAuditedEvents,omitempty"`

	// lastReconcileTime is when the controller last reconciled this resource.
	// +optional
	LastReconcileTime *metav1.Time `json:"lastReconcileTime,omitempty"`

	// podStatuses contains detailed status for each guarded pod.
	// +optional
	// +listType=map
	// +listMapKey=name
	PodStatuses []GuardedPodStatus `json:"podStatuses,omitempty"`

	// conditions represent the current state of the JanusGuard resource.
	// Standard condition types: Available, Progressing, Degraded
	// +listType=map
	// +listMapKey=type
	// +optional
	Conditions []metav1.Condition `json:"conditions,omitempty"`
}

// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
// +kubebuilder:storageversion
// +kubebuilder:resource:shortName=jg,categories=all;janus;security
// +kubebuilder:printcolumn:name="Observable",type=integer,JSONPath=`.status.observablePods`,description="Number of pods matching selector"
// +kubebuilder:printcolumn:name="Guarded",type=integer,JSONPath=`.status.guardedPods`,description="Number of pods being guarded"
// +kubebuilder:printcolumn:name="Denied",type=integer,JSONPath=`.status.totalDeniedEvents`,description="Total denied access attempts"
// +kubebuilder:printcolumn:name="Ready",type=boolean,JSONPath=`.status.marksRegistered`,description="All marks registered"
// +kubebuilder:printcolumn:name="Enforcing",type=boolean,JSONPath=`.spec.enforcing`,description="Whether denials are enforced"
// +kubebuilder:printcolumn:name="Age",type=date,JSONPath=`.metadata.creationTimestamp`

// JanusGuard is the Schema for the janusguards API.
// It defines file access auditing and control rules for pods matching a selector.
// This is the v2 API - the current recommended version for new deployments.
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

// Hub marks this type as the conversion hub.
func (*JanusGuard) Hub() {}

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
