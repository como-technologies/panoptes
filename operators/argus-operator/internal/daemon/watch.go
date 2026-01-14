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

package daemon

import (
	"context"
	"fmt"
	"io"
	"strings"

	corev1 "k8s.io/api/core/v1"
	"sigs.k8s.io/controller-runtime/pkg/log"

	arguspb "github.com/como-technologies/panoptes/gen/go/argus/v2"
	argusv2 "github.com/como-technologies/panoptes/operators/argus-operator/api/v2"
)

// requestTimeout wraps a context with the default request timeout.
func requestTimeout(ctx context.Context) (context.Context, context.CancelFunc) {
	return context.WithTimeout(ctx, DefaultRequestTimeout)
}

// WatchConfig contains configuration for creating a watch on a daemon.
type WatchConfig struct {
	WatcherName      string
	WatcherNamespace string
	NodeName         string
	NodeIP           string
	PodName          string
	PodNamespace     string
	ContainerIDs     []string
	PIDs             []int32
	Subjects         []argusv2.ArgusWatcherSubject
	LogFormat        string
	Paused           bool // Whether the watcher is paused
}

// WatchResult contains the result of a watch operation.
type WatchResult struct {
	WatchID          string
	WatchDescriptors int32
	Success          bool
	Error            error
}

// WatchState represents the actual state of a watch on the daemon.
type WatchState struct {
	WatcherName  string
	Namespace    string
	NodeName     string
	PodName      string
	PIDs         []int32
	WatchedPaths int32
	Paused       bool
	// For config comparison
	Subjects  []WatchSubjectState
	LogFormat string
}

// WatchSubjectState represents a subject's state for comparison.
type WatchSubjectState struct {
	Paths     []string
	Events    []string
	Recursive bool
	MaxDepth  int32
}

// WatchManager manages file watches on argusd daemons.
type WatchManager struct {
	client *Client
}

// NewWatchManager creates a new watch manager.
func NewWatchManager(client *Client) *WatchManager {
	return &WatchManager{
		client: client,
	}
}

// CreateWatch creates a new file watch on the daemon for the specified pod.
func (m *WatchManager) CreateWatch(ctx context.Context, config *WatchConfig) (*WatchResult, error) {
	logger := log.FromContext(ctx).WithValues(
		"watcher", config.WatcherName,
		"pod", config.PodName,
		"node", config.NodeName,
	)

	conn, err := m.client.GetConnection(ctx, config.NodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection to node %s: %w", config.NodeName, err)
	}

	// Convert subjects to proto format
	protoSubjects := make([]*arguspb.WatchSubject, len(config.Subjects))
	for i, subject := range config.Subjects {
		events := make([]arguspb.InotifyEvent, len(subject.Events))
		for j, event := range subject.Events {
			events[j] = convertEventToProto(event)
		}

		var maxDepth int32
		if subject.MaxDepth != nil {
			maxDepth = *subject.MaxDepth
		}

		protoSubjects[i] = &arguspb.WatchSubject{
			Paths:      subject.Paths,
			Events:     events,
			Ignore:     subject.Ignore,
			Recursive:  subject.Recursive,
			MaxDepth:   maxDepth,
			OnlyDir:    subject.OnlyDir,
			FollowMove: subject.FollowMove,
			Tags:       subject.Tags,
		}
	}

	logger.V(1).Info("Creating watch",
		"containerIDs", config.ContainerIDs,
		"pids", config.PIDs,
		"subjectCount", len(config.Subjects),
	)

	// Create gRPC client and make the call
	client := arguspb.NewArgusdServiceClient(conn)
	req := &arguspb.CreateWatchRequest{
		WatcherName:  config.WatcherName,
		Namespace:    config.WatcherNamespace,
		NodeName:     config.NodeName,
		PodName:      config.PodName,
		ContainerIds: config.ContainerIDs,
		Pids:         config.PIDs,
		Subjects:     protoSubjects,
		LogFormat:    config.LogFormat,
		Paused:       config.Paused,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	resp, err := client.CreateWatch(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC CreateWatch failed: %w", err)
	}

	logger.Info("Watch created successfully",
		"watchID", resp.WatchId,
		"watchedPaths", resp.WatchedPaths,
		"paused", resp.Paused,
	)

	return &WatchResult{
		WatchID:          resp.WatchId,
		WatchDescriptors: resp.WatchedPaths,
		Success:          true,
	}, nil
}

// DestroyWatch removes a file watch from the daemon.
func (m *WatchManager) DestroyWatch(ctx context.Context, nodeIP, watcherNamespace, watcherName, podName string) error {
	logger := log.FromContext(ctx).WithValues(
		"watcher", watcherName,
		"pod", podName,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return fmt.Errorf("failed to get connection: %w", err)
	}

	logger.V(1).Info("Destroying watch")

	// Create gRPC client and make the call
	client := arguspb.NewArgusdServiceClient(conn)
	req := &arguspb.DestroyWatchRequest{
		WatcherName: watcherName,
		Namespace:   watcherNamespace,
		PodName:     podName,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	_, err = client.DestroyWatch(reqCtx, req)
	if err != nil {
		return fmt.Errorf("gRPC DestroyWatch failed: %w", err)
	}

	logger.Info("Watch destroyed successfully")
	return nil
}

// GetWatchState queries the daemon for the actual state of watches.
// This is used to implement the query-first pattern for reconciliation.
func (m *WatchManager) GetWatchState(ctx context.Context, nodeIP, watcherName, namespace string) ([]WatchState, error) {
	logger := log.FromContext(ctx).WithValues(
		"watcher", watcherName,
		"namespace", namespace,
		"nodeIP", nodeIP,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection: %w", err)
	}

	client := arguspb.NewArgusdServiceClient(conn)
	req := &arguspb.GetWatchStateRequest{
		WatcherName: watcherName,
		Namespace:   namespace,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	stream, err := client.GetWatchState(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC GetWatchState failed: %w", err)
	}

	var watches []WatchState
	for {
		state, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("error receiving watch state: %w", err)
		}

		// Convert proto subjects to local type
		subjects := make([]WatchSubjectState, len(state.Subjects))
		for i, s := range state.Subjects {
			events := make([]string, len(s.Events))
			for j, e := range s.Events {
				events[j] = e.String()
			}
			subjects[i] = WatchSubjectState{
				Paths:     s.Paths,
				Events:    events,
				Recursive: s.Recursive,
				MaxDepth:  s.MaxDepth,
			}
		}

		watches = append(watches, WatchState{
			WatcherName:  state.WatcherName,
			Namespace:    state.Namespace,
			NodeName:     state.NodeName,
			PodName:      state.PodName,
			PIDs:         state.Pids,
			WatchedPaths: state.WatchDescriptors,
			Paused:       state.Paused,
			Subjects:     subjects,
			LogFormat:    state.LogFormat,
		})
	}

	logger.V(1).Info("Retrieved watch states", "count", len(watches))
	return watches, nil
}

// convertEventToProto converts an ArgusEvent to the proto InotifyEvent enum.
func convertEventToProto(event argusv2.ArgusEvent) arguspb.InotifyEvent {
	// Normalize event string (lowercase, remove underscores)
	eventStr := strings.ToLower(string(event))
	eventStr = strings.ReplaceAll(eventStr, "_", "")

	switch eventStr {
	case "access":
		return arguspb.InotifyEvent_INOTIFY_EVENT_ACCESS
	case "attrib":
		return arguspb.InotifyEvent_INOTIFY_EVENT_ATTRIB
	case "closewrite":
		return arguspb.InotifyEvent_INOTIFY_EVENT_CLOSE_WRITE
	case "closenowrite":
		return arguspb.InotifyEvent_INOTIFY_EVENT_CLOSE_NOWRITE
	case "create":
		return arguspb.InotifyEvent_INOTIFY_EVENT_CREATE
	case "delete":
		return arguspb.InotifyEvent_INOTIFY_EVENT_DELETE
	case "deleteself":
		return arguspb.InotifyEvent_INOTIFY_EVENT_DELETE_SELF
	case "modify":
		return arguspb.InotifyEvent_INOTIFY_EVENT_MODIFY
	case "moveself":
		return arguspb.InotifyEvent_INOTIFY_EVENT_MOVE_SELF
	case "movedfrom":
		return arguspb.InotifyEvent_INOTIFY_EVENT_MOVED_FROM
	case "movedto":
		return arguspb.InotifyEvent_INOTIFY_EVENT_MOVED_TO
	case "open":
		return arguspb.InotifyEvent_INOTIFY_EVENT_OPEN
	case "all":
		return arguspb.InotifyEvent_INOTIFY_EVENT_ALL
	default:
		return arguspb.InotifyEvent_INOTIFY_EVENT_UNSPECIFIED
	}
}

// GetContainerIDs extracts container IDs from a pod.
func GetContainerIDs(pod *corev1.Pod) []string {
	var containerIDs []string
	for _, status := range pod.Status.ContainerStatuses {
		if status.ContainerID != "" {
			containerIDs = append(containerIDs, status.ContainerID)
		}
	}
	return containerIDs
}

// GetNodeIP returns the internal IP of a node.
func GetNodeIP(node *corev1.Node) string {
	for _, addr := range node.Status.Addresses {
		if addr.Type == corev1.NodeInternalIP {
			return addr.Address
		}
	}
	return ""
}

// UpdateWatchResult contains the result of an update watch operation (v2 API).
type UpdateWatchResult struct {
	WatchID      string
	Paused       bool
	WatchedPaths int32
}

// UpdateWatch pauses or resumes an existing watch (v2 API).
// This method uses the v2 proto which is only supported by Rust daemons.
func (m *WatchManager) UpdateWatch(ctx context.Context, nodeIP, watcherNamespace, watcherName, podName string, pause bool) (*UpdateWatchResult, error) {
	logger := log.FromContext(ctx).WithValues(
		"watcher", watcherName,
		"namespace", watcherNamespace,
		"pod", podName,
		"pause", pause,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection: %w", err)
	}

	action := arguspb.UpdateAction_UPDATE_ACTION_RESUME
	if pause {
		action = arguspb.UpdateAction_UPDATE_ACTION_PAUSE
	}

	logger.V(1).Info("Updating watch state")

	// Create gRPC client and make the call
	client := arguspb.NewArgusdServiceClient(conn)
	req := &arguspb.UpdateWatchRequest{
		WatcherName: watcherName,
		Namespace:   watcherNamespace,
		PodName:     podName,
		Action:      action,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	resp, err := client.UpdateWatch(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC UpdateWatch failed: %w", err)
	}

	logger.Info("Watch updated successfully",
		"watchID", resp.WatchId,
		"paused", resp.Paused,
		"watchedPaths", resp.WatchedPaths,
	)

	return &UpdateWatchResult{
		WatchID:      resp.WatchId,
		Paused:       resp.Paused,
		WatchedPaths: resp.WatchedPaths,
	}, nil
}
