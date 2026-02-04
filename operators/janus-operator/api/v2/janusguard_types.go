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

// JanusEvent represents a Linux fanotify event type for file access auditing.
// These map to fanotify_mark(2) event flags. Unlike inotify, fanotify can
// block access (when enforcing is true), not just detect it.
// +kubebuilder:validation:Enum=access;open;execute;close;all
type JanusEvent string

const (
	// EventAccess fires when a file is read. Maps to FAN_ACCESS_PERM (enforcing) or FAN_ACCESS (audit).
	EventAccess JanusEvent = "access"
	// EventOpen fires when a file is opened. Maps to FAN_OPEN_PERM (enforcing) or FAN_OPEN (audit).
	EventOpen JanusEvent = "open"
	// EventExecute fires when a file is executed. Maps to FAN_OPEN_EXEC_PERM. Requires kernel 5.0+.
	EventExecute JanusEvent = "execute"
	// EventClose fires when a file descriptor is closed. Maps to FAN_CLOSE.
	EventClose JanusEvent = "close"
	// EventAll monitors all event types. Generates high event volume; use specific events in production.
	EventAll JanusEvent = "all"
)

// JanusResponse represents the action to take when a file access event is detected.
// When spec.enforcing is false, deny responses are logged but access is still allowed (dry-run).
// +kubebuilder:validation:Enum=allow;deny;audit
type JanusResponse string

const (
	// ResponseAllow permits access and records the event.
	ResponseAllow JanusResponse = "allow"
	// ResponseDeny blocks access with EACCES. Only enforced when spec.enforcing is true.
	ResponseDeny JanusResponse = "deny"
	// ResponseAudit records the event without affecting access. Use for monitoring without impact.
	ResponseAudit JanusResponse = "audit"
)

// ContainerRuntime specifies the container runtime used for resolving container
// PIDs to filesystem paths. The daemon accesses container filesystems via /proc/{pid}/root.
// +kubebuilder:validation:Enum=containerd;cri-o;auto
type ContainerRuntime string

const (
	// RuntimeContainerd uses the containerd socket for container PID resolution.
	RuntimeContainerd ContainerRuntime = "containerd"
	// RuntimeCRIO uses the CRI-O socket for container PID resolution.
	RuntimeCRIO ContainerRuntime = "cri-o"
	// RuntimeAuto detects the container runtime automatically by probing known socket paths.
	RuntimeAuto ContainerRuntime = "auto"
)

// JanusGuardSubject defines file access control rules for a set of paths.
// Each subject evaluates deny rules first, then allow rules, then falls back to defaultResponse.
// Multiple subjects can target different paths with independent rules and tags.
type JanusGuardSubject struct {
	// allow is the list of paths to explicitly allow access to.
	// Paths support glob patterns (e.g., "/app/**", "/etc/*.conf").
	// Allow rules are evaluated after deny rules; if a path matches both, deny wins.
	// +optional
	// +kubebuilder:validation:MaxItems=100
	Allow []string `json:"allow,omitempty"`

	// deny is the list of paths to block access to.
	// Paths support glob patterns (e.g., "/etc/shadow", "/var/run/docker.sock").
	// Deny rules take precedence over allow rules. When spec.enforcing is true,
	// access is blocked with EACCES. When false, access is logged but permitted.
	// +optional
	// +kubebuilder:validation:MaxItems=100
	Deny []string `json:"deny,omitempty"`

	// events is the list of fanotify event types to monitor.
	// Use "all" to monitor all event types. For access control, "open" and "access"
	// are the most common. "execute" requires kernel 5.0+.
	// +kubebuilder:validation:MinItems=1
	Events []JanusEvent `json:"events"`

	// onlyDir restricts fanotify marks to directories only.
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// autoAllowOwner automatically allows access when the accessing process UID
	// matches the file owner UID. Useful for applications that need to read their
	// own config files or Kubernetes service account tokens.
	// +optional
	AutoAllowOwner bool `json:"autoAllowOwner,omitempty"`

	// audit enables kernel audit log integration via AUDIT_WRITE.
	// When true, access events are written to the kernel audit log in addition
	// to being reported via gRPC. Useful for compliance frameworks requiring
	// kernel-level audit trails (NIST 800-53 AU-2).
	// +optional
	Audit bool `json:"audit,omitempty"`

	// defaultResponse is the action taken when a file access matches no allow or deny rule.
	// "audit" (default) logs without affecting access. "allow" silently permits.
	// "deny" blocks access (only when spec.enforcing is true).
	// +kubebuilder:validation:Enum=allow;deny;audit
	// +kubebuilder:default=audit
	// +optional
	DefaultResponse JanusResponse `json:"defaultResponse,omitempty"`

	// tags are custom key-value metadata attached to events from this subject.
	// Useful for compliance tracking (e.g., "compliance": "pci-dss", "severity": "critical"),
	// alert routing, or categorization in downstream SIEM systems.
	// +optional
	// +kubebuilder:validation:MaxProperties=20
	Tags map[string]string `json:"tags,omitempty"`
}

// JanusGuardSpec defines the desired state of JanusGuard.
// The operator watches for pods matching the selector and instructs the janusd
// daemon (via gRPC on port 50052) to create fanotify marks on each pod's filesystem.
// Requires: janusd DaemonSet with CAP_SYS_ADMIN, CAP_SYS_PTRACE, and CAP_DAC_READ_SEARCH.
type JanusGuardSpec struct {
	// selector is the label selector for pods to guard.
	// Only pods matching this selector will have fanotify access control applied.
	// Use matchLabels for simple key-value matching, or matchExpressions for
	// set-based requirements. Example: {"matchLabels": {"pci-dss/scope": "in-scope"}}.
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of access control rules to apply.
	// Each subject defines independent allow/deny rules for file access paths.
	// Use multiple subjects to apply different policies with different tags.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=20
	Subjects []JanusGuardSubject `json:"subjects"`

	// containerRuntime specifies which container runtime to use for PID detection.
	// "auto" probes for containerd and CRI-O sockets in standard locations.
	// Set explicitly if auto-detection fails or if using a non-standard socket path.
	// +kubebuilder:validation:Enum=containerd;cri-o;auto
	// +kubebuilder:default=auto
	// +optional
	ContainerRuntime ContainerRuntime `json:"containerRuntime,omitempty"`

	// logFormat is a custom Go template for formatting event log output.
	// Available fields: .Event, .Path, .Response, .Timestamp, .Pod, .Node.
	// Leave empty to use the default structured JSON format.
	// +optional
	// +kubebuilder:validation:MaxLength=1024
	LogFormat string `json:"logFormat,omitempty"`

	// paused temporarily suspends all access control for this resource.
	// When true, all fanotify marks are removed and no access control is applied.
	// Use this during planned maintenance windows or emergency response.
	// Set back to false to resume guarding.
	// +optional
	Paused bool `json:"paused,omitempty"`

	// enforcing controls whether deny rules actively block file access.
	// When true (default), denied paths return EACCES to the accessing process.
	// When false (dry-run mode), denials are logged but access is permitted.
	// IMPORTANT: Start with enforcing=false in production to validate rules
	// before enabling enforcement. See docs/guides/enabling-enforcement.md.
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

// JanusGuard is the Schema for the janusguards API (short name: jg).
// It defines file access auditing and enforcement rules using Linux fanotify for pods
// matching a label selector. The janus-operator watches JanusGuard resources and instructs
// the janusd daemon on each node to create kernel-level access control marks.
//
// Unlike ArgusWatcher (which detects changes), JanusGuard can actively block file access
// when spec.enforcing is true. This makes it suitable for runtime protection scenarios
// like blocking container runtime socket access or protecting sensitive credentials.
//
// Requires: janusd DaemonSet with CAP_SYS_ADMIN (fanotify), CAP_SYS_PTRACE, CAP_DAC_READ_SEARCH.
// Kernel requirements: Linux 5.x+ with fanotify support.
//
// Quick example:
//
//	kubectl apply -f - <<EOF
//	apiVersion: janus.panoptes.io/v2
//	kind: JanusGuard
//	metadata:
//	  name: block-runtime-sockets
//	spec:
//	  enforcing: true
//	  selector:
//	    matchLabels:
//	      app: my-app
//	  subjects:
//	    - deny: ["/var/run/docker.sock", "/run/containerd/containerd.sock"]
//	      events: [open, access]
//	      tags:
//	        severity: critical
//	        compliance: cis-kubernetes
//	EOF
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
