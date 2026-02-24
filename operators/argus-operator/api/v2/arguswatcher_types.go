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

// ArgusEvent represents a Linux inotify event type for file integrity monitoring.
// These map directly to inotify_add_watch(2) event flags.
// For compliance use cases, "create", "modify", "delete", and "move" cover most requirements.
// +kubebuilder:validation:Enum=access;attrib;closewrite;closenowrite;close;create;delete;deleteself;modify;moveself;movedfrom;movedto;move;open;all
type ArgusEvent string

const (
	// EventAccess fires when a file is read (e.g., cat, head). Maps to IN_ACCESS.
	EventAccess ArgusEvent = "access"
	// EventAttrib fires when file metadata changes (permissions, ownership, timestamps). Maps to IN_ATTRIB.
	EventAttrib ArgusEvent = "attrib"
	// EventCloseWrite fires when a writable file descriptor is closed. Maps to IN_CLOSE_WRITE.
	EventCloseWrite ArgusEvent = "closewrite"
	// EventCloseNoWrite fires when a read-only file descriptor is closed. Maps to IN_CLOSE_NOWRITE.
	EventCloseNoWrite ArgusEvent = "closenowrite"
	// EventClose fires on any file close (combines closewrite + closenowrite). Maps to IN_CLOSE.
	EventClose ArgusEvent = "close"
	// EventCreate fires when a new file or directory is created in a watched directory. Maps to IN_CREATE.
	EventCreate ArgusEvent = "create"
	// EventDelete fires when a file or directory is deleted from a watched directory. Maps to IN_DELETE.
	EventDelete ArgusEvent = "delete"
	// EventDeleteSelf fires when the watched file or directory itself is deleted. Maps to IN_DELETE_SELF.
	EventDeleteSelf ArgusEvent = "deleteself"
	// EventModify fires when file content is written to. Maps to IN_MODIFY.
	EventModify ArgusEvent = "modify"
	// EventMoveSelf fires when the watched file or directory itself is moved. Maps to IN_MOVE_SELF.
	EventMoveSelf ArgusEvent = "moveself"
	// EventMovedFrom fires when a file is moved out of a watched directory. Maps to IN_MOVED_FROM.
	EventMovedFrom ArgusEvent = "movedfrom"
	// EventMovedTo fires when a file is moved into a watched directory. Maps to IN_MOVED_TO.
	EventMovedTo ArgusEvent = "movedto"
	// EventMove fires on any file move (combines movedfrom + movedto). Maps to IN_MOVE.
	EventMove ArgusEvent = "move"
	// EventOpen fires when a file is opened. Maps to IN_OPEN.
	EventOpen ArgusEvent = "open"
	// EventAll watches for all event types. Generates high event volume; use specific events in production.
	EventAll ArgusEvent = "all"
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

// ArgusWatcherSubject defines a watch target with paths and events.
// Each subject creates inotify watches on the specified paths for the specified events.
// Multiple subjects can be used to watch different paths with different event types and tags.
type ArgusWatcherSubject struct {
	// paths is the list of file or directory paths to monitor.
	// Paths are resolved within the container's filesystem via /proc/{pid}/root.
	// Supports absolute paths only (e.g., "/etc/passwd", "/var/log").
	// Note: Kubernetes ConfigMap/Secret mounts use symlinks that are atomically swapped.
	// To catch ConfigMap updates, watch the parent directory rather than individual files.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=100
	Paths []string `json:"paths"`

	// events is the list of inotify events to watch for.
	// Use "all" to watch for all event types (generates high volume).
	// For compliance, ["create", "modify", "delete", "move"] covers most requirements.
	// +kubebuilder:validation:MinItems=1
	Events []ArgusEvent `json:"events"`

	// ignore is the list of glob patterns to exclude from monitoring.
	// Patterns follow standard glob syntax (e.g., "*.tmp", "*.swp", "**/node_modules/**").
	// Use this to reduce noise from known-safe file operations.
	// +optional
	// +kubebuilder:validation:MaxItems=50
	Ignore []string `json:"ignore,omitempty"`

	// recursive enables recursive directory watching.
	// When true, all subdirectories under each path are also monitored.
	// Each subdirectory consumes one inotify watch descriptor. Monitor kernel limits
	// via /proc/sys/fs/inotify/max_user_watches (recommended: 524288).
	// +optional
	Recursive bool `json:"recursive,omitempty"`

	// maxDepth limits the recursion depth when recursive is enabled.
	// A value of 0 means unlimited depth. Only effective when recursive is true.
	// Use this to prevent excessive watch descriptor consumption in deep directory trees.
	// +optional
	// +kubebuilder:validation:Minimum=0
	// +kubebuilder:validation:Maximum=100
	MaxDepth *int32 `json:"maxDepth,omitempty"`

	// onlyDir restricts watching to directories only (maps to IN_ONLYDIR).
	// File events within watched directories are still reported.
	// +optional
	OnlyDir bool `json:"onlyDir,omitempty"`

	// followMove tracks moved files by inode across watched paths.
	// When true, a file moved from one watched directory to another is correlated
	// into a single move event rather than separate delete + create events.
	// +optional
	FollowMove bool `json:"followMove,omitempty"`

	// tags are custom key-value metadata attached to events from this subject.
	// Useful for compliance tracking (e.g., "compliance": "pci-dss", "severity": "critical"),
	// alert routing, or categorization in downstream SIEM systems.
	// +optional
	// +kubebuilder:validation:MaxProperties=20
	Tags map[string]string `json:"tags,omitempty"`

	// skipIfMissing disables automatic proxy watching for paths that do not
	// exist at watch creation time. When false (default), the daemon watches
	// the nearest ancestor directory and promotes to a direct watch when the
	// target appears. When true, non-existent paths are silently skipped.
	// +optional
	SkipIfMissing bool `json:"skipIfMissing,omitempty"`
}

// ArgusWatcherSpec defines the desired state of ArgusWatcher.
// The operator watches for pods matching the selector and instructs the argusd
// daemon (via gRPC on port 50051) to create inotify watches on each pod's filesystem.
type ArgusWatcherSpec struct {
	// selector is the label selector for pods to monitor.
	// Only pods matching this selector will have inotify watches created.
	// Use matchLabels for simple key-value matching, or matchExpressions for
	// set-based requirements. Example: {"matchLabels": {"app": "payment-api"}}.
	// +kubebuilder:validation:Required
	Selector metav1.LabelSelector `json:"selector"`

	// subjects is the list of watch targets defining what paths and events to monitor.
	// Each subject creates independent inotify watches with its own paths, events, and tags.
	// Use multiple subjects to apply different monitoring rules to the same set of pods.
	// +kubebuilder:validation:MinItems=1
	// +kubebuilder:validation:MaxItems=20
	Subjects []ArgusWatcherSubject `json:"subjects"`

	// containerRuntime specifies which container runtime to use for PID detection.
	// "auto" probes for containerd and CRI-O sockets in standard locations.
	// Set explicitly if auto-detection fails or if using a non-standard socket path.
	// +kubebuilder:validation:Enum=containerd;cri-o;auto
	// +kubebuilder:default=auto
	// +optional
	ContainerRuntime ContainerRuntime `json:"containerRuntime,omitempty"`

	// logFormat is a custom Go template for formatting event log output.
	// Available fields: .Event, .Path, .Timestamp, .Pod, .Node.
	// Leave empty to use the default structured JSON format.
	// +optional
	// +kubebuilder:validation:MaxLength=1024
	LogFormat string `json:"logFormat,omitempty"`

	// paused temporarily suspends all monitoring for this resource.
	// When true, no new watches are created and existing watches are removed.
	// Use this during planned maintenance windows to avoid false alerts.
	// Set back to false to resume monitoring.
	// +optional
	Paused bool `json:"paused"`
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

	// watchesReady indicates watches are active for this pod.
	// +optional
	WatchesReady bool `json:"watchesReady"`

	// readyAt is when watches became ready for this pod.
	// +optional
	ReadyAt *metav1.Time `json:"readyAt,omitempty"`

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

	// watchesReady indicates all inotify watches are registered and active.
	// +optional
	WatchesReady bool `json:"watchesReady"`

	// readyAt is when the watcher became fully ready (all watches registered).
	// +optional
	ReadyAt *metav1.Time `json:"readyAt,omitempty"`

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
// +kubebuilder:printcolumn:name="Ready",type=boolean,JSONPath=`.status.watchesReady`,description="All watches registered"
// +kubebuilder:printcolumn:name="Paused",type=boolean,JSONPath=`.spec.paused`,description="Whether watching is paused"
// +kubebuilder:printcolumn:name="Age",type=date,JSONPath=`.metadata.creationTimestamp`

// ArgusWatcher is the Schema for the arguswatchers API (short name: aw).
// It defines file integrity monitoring (FIM) rules using Linux inotify for pods matching
// a label selector. The argus-operator watches ArgusWatcher resources and instructs
// the argusd daemon on each node to create kernel-level file watches.
//
// Requires: argusd DaemonSet with CAP_SYS_PTRACE and CAP_DAC_READ_SEARCH capabilities.
// Kernel requirements: Linux 5.x+, inotify support (standard on all modern kernels).
//
// Quick example:
//
//	kubectl apply -f - <<EOF
//	apiVersion: argus.panoptes.io/v2
//	kind: ArgusWatcher
//	metadata:
//	  name: critical-files
//	spec:
//	  selector:
//	    matchLabels:
//	      app: my-app
//	  subjects:
//	    - paths: ["/etc/passwd", "/etc/shadow"]
//	      events: [create, modify, delete]
//	      tags:
//	        severity: critical
//	EOF
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

// Hub marks this type as the conversion hub.
func (*ArgusWatcher) Hub() {}

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
