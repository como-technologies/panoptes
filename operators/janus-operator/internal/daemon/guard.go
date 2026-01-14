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

	januspb "github.com/como-technologies/panoptes/gen/go/janus/v2"
	janusv2 "github.com/como-technologies/panoptes/operators/janus-operator/api/v2"
)

// requestTimeout wraps a context with the default request timeout.
func requestTimeout(ctx context.Context) (context.Context, context.CancelFunc) {
	return context.WithTimeout(ctx, DefaultRequestTimeout)
}

// GuardConfig contains configuration for creating a guard on a daemon.
type GuardConfig struct {
	GuardName      string
	GuardNamespace string
	NodeName       string
	NodeIP         string
	PodName        string
	PodNamespace   string
	ContainerIDs   []string
	PIDs           []int32
	Subjects       []janusv2.JanusGuardSubject
	LogFormat      string
	Paused         bool // Whether the guard is paused
	Enforcing      bool // Whether access denials are enforced (vs audit mode)
}

// GuardResult contains the result of a guard operation.
type GuardResult struct {
	GuardID      string
	GuardedPaths int32
	Success      bool
	Error        error
}

// GuardState represents the actual state of a guard on the daemon.
type GuardState struct {
	GuardName    string
	Namespace    string
	NodeName     string
	PodName      string
	PIDs         []int32
	GuardedPaths int32
	Paused       bool
	Enforcing    bool
	// For config comparison
	Subjects  []GuardSubjectState
	LogFormat string
}

// GuardSubjectState represents a subject's state for comparison.
type GuardSubjectState struct {
	Allow  []string
	Deny   []string
	Events []string
}

// GuardManager manages file access guards on janusd daemons.
type GuardManager struct {
	client *Client
}

// NewGuardManager creates a new guard manager.
func NewGuardManager(client *Client) *GuardManager {
	return &GuardManager{
		client: client,
	}
}

// CreateGuard creates a new file access guard on the daemon for the specified pod.
func (m *GuardManager) CreateGuard(ctx context.Context, config *GuardConfig) (*GuardResult, error) {
	logger := log.FromContext(ctx).WithValues(
		"guard", config.GuardName,
		"pod", config.PodName,
		"node", config.NodeName,
	)

	conn, err := m.client.GetConnection(ctx, config.NodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection to node %s: %w", config.NodeName, err)
	}

	// Convert subjects to proto format
	protoSubjects := make([]*januspb.GuardSubject, len(config.Subjects))
	for i, subject := range config.Subjects {
		events := make([]januspb.FanotifyEvent, len(subject.Events))
		for j, event := range subject.Events {
			events[j] = convertEventToProto(event)
		}

		protoSubjects[i] = &januspb.GuardSubject{
			Allow:           subject.Allow,
			Deny:            subject.Deny,
			Events:          events,
			OnlyDir:         subject.OnlyDir,
			AutoAllowOwner:  subject.AutoAllowOwner,
			Audit:           subject.Audit,
			DefaultResponse: convertResponseToProto(subject.DefaultResponse),
			Tags:            subject.Tags,
		}
	}

	logger.V(1).Info("Creating guard",
		"containerIDs", config.ContainerIDs,
		"pids", config.PIDs,
		"subjectCount", len(config.Subjects),
		"enforcing", config.Enforcing,
	)

	// Create gRPC client and make the call
	client := januspb.NewJanusdServiceClient(conn)
	req := &januspb.CreateGuardRequest{
		GuardName:    config.GuardName,
		Namespace:    config.GuardNamespace,
		NodeName:     config.NodeName,
		PodName:      config.PodName,
		ContainerIds: config.ContainerIDs,
		Pids:         config.PIDs,
		Subjects:     protoSubjects,
		LogFormat:    config.LogFormat,
		Paused:       config.Paused,
		Enforcing:    config.Enforcing,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	resp, err := client.CreateGuard(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC CreateGuard failed: %w", err)
	}

	logger.Info("Guard created successfully",
		"guardID", resp.GuardId,
		"guardedPaths", resp.GuardedPaths,
		"paused", resp.Paused,
		"enforcing", resp.Enforcing,
	)

	return &GuardResult{
		GuardID:      resp.GuardId,
		GuardedPaths: resp.GuardedPaths,
		Success:      true,
	}, nil
}

// DestroyGuard removes a file access guard from the daemon.
func (m *GuardManager) DestroyGuard(ctx context.Context, nodeIP, guardNamespace, guardName, podName string) error {
	logger := log.FromContext(ctx).WithValues(
		"guard", guardName,
		"pod", podName,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return fmt.Errorf("failed to get connection: %w", err)
	}

	logger.V(1).Info("Destroying guard")

	// Create gRPC client and make the call
	client := januspb.NewJanusdServiceClient(conn)
	req := &januspb.DestroyGuardRequest{
		GuardName: guardName,
		Namespace: guardNamespace,
		PodName:   podName,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	_, err = client.DestroyGuard(reqCtx, req)
	if err != nil {
		return fmt.Errorf("gRPC DestroyGuard failed: %w", err)
	}

	logger.Info("Guard destroyed successfully")
	return nil
}

// GetGuardState queries the daemon for the actual state of guards.
// This is used to implement the query-first pattern for reconciliation.
func (m *GuardManager) GetGuardState(ctx context.Context, nodeIP, guardName, namespace string) ([]GuardState, error) {
	logger := log.FromContext(ctx).WithValues(
		"guard", guardName,
		"namespace", namespace,
		"nodeIP", nodeIP,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection: %w", err)
	}

	client := januspb.NewJanusdServiceClient(conn)
	req := &januspb.GetGuardStateRequest{
		GuardName: guardName,
		Namespace: namespace,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	stream, err := client.GetGuardState(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC GetGuardState failed: %w", err)
	}

	var guards []GuardState
	for {
		state, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("error receiving guard state: %w", err)
		}

		// Convert proto subjects to local type
		subjects := make([]GuardSubjectState, len(state.Subjects))
		for i, s := range state.Subjects {
			events := make([]string, len(s.Events))
			for j, e := range s.Events {
				events[j] = e.String()
			}
			subjects[i] = GuardSubjectState{
				Allow:  s.Allow,
				Deny:   s.Deny,
				Events: events,
			}
		}

		guards = append(guards, GuardState{
			GuardName:    state.GuardName,
			Namespace:    state.Namespace,
			NodeName:     state.NodeName,
			PodName:      state.PodName,
			PIDs:         state.Pids,
			GuardedPaths: state.GuardedPaths,
			Paused:       state.Paused,
			Enforcing:    state.Enforcing,
			Subjects:     subjects,
			LogFormat:    state.LogFormat,
		})
	}

	logger.V(1).Info("Retrieved guard states", "count", len(guards))
	return guards, nil
}

// convertEventToProto converts a JanusEvent to the proto FanotifyEvent enum.
func convertEventToProto(event janusv2.JanusEvent) januspb.FanotifyEvent {
	eventStr := strings.ToLower(string(event))

	switch eventStr {
	case "access":
		return januspb.FanotifyEvent_FANOTIFY_EVENT_ACCESS
	case "open":
		return januspb.FanotifyEvent_FANOTIFY_EVENT_OPEN
	case "execute":
		return januspb.FanotifyEvent_FANOTIFY_EVENT_OPEN_EXEC
	case "close":
		return januspb.FanotifyEvent_FANOTIFY_EVENT_CLOSE
	case "all":
		return januspb.FanotifyEvent_FANOTIFY_EVENT_ALL
	default:
		return januspb.FanotifyEvent_FANOTIFY_EVENT_UNSPECIFIED
	}
}

// convertResponseToProto converts a JanusResponse to the proto AccessResponse enum.
func convertResponseToProto(response janusv2.JanusResponse) januspb.AccessResponse {
	responseStr := strings.ToLower(string(response))

	switch responseStr {
	case "allow":
		return januspb.AccessResponse_ACCESS_RESPONSE_ALLOW
	case "deny":
		return januspb.AccessResponse_ACCESS_RESPONSE_DENY
	case "audit":
		return januspb.AccessResponse_ACCESS_RESPONSE_AUDIT
	default:
		return januspb.AccessResponse_ACCESS_RESPONSE_UNSPECIFIED
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

// UpdateGuardResult contains the result of an update guard operation (v2 API).
type UpdateGuardResult struct {
	GuardID      string
	Paused       bool
	Enforcing    bool
	GuardedPaths int32
}

// UpdateGuard pauses or resumes an existing guard (v2 API).
// This method uses the v2 proto which is only supported by Rust daemons.
func (m *GuardManager) UpdateGuard(ctx context.Context, nodeIP, guardNamespace, guardName, podName string, pause bool) (*UpdateGuardResult, error) {
	logger := log.FromContext(ctx).WithValues(
		"guard", guardName,
		"namespace", guardNamespace,
		"pod", podName,
		"pause", pause,
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection: %w", err)
	}

	action := januspb.UpdateAction_UPDATE_ACTION_RESUME
	if pause {
		action = januspb.UpdateAction_UPDATE_ACTION_PAUSE
	}

	logger.V(1).Info("Updating guard state")

	// Create gRPC client and make the call
	client := januspb.NewJanusdServiceClient(conn)
	req := &januspb.UpdateGuardRequest{
		GuardName: guardName,
		Namespace: guardNamespace,
		PodName:   podName,
		Action:    action,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	resp, err := client.UpdateGuard(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC UpdateGuard failed: %w", err)
	}

	logger.Info("Guard updated successfully",
		"guardID", resp.GuardId,
		"paused", resp.Paused,
		"enforcing", resp.Enforcing,
		"guardedPaths", resp.GuardedPaths,
	)

	return &UpdateGuardResult{
		GuardID:      resp.GuardId,
		Paused:       resp.Paused,
		Enforcing:    resp.Enforcing,
		GuardedPaths: resp.GuardedPaths,
	}, nil
}

// UpdatePolicyResult contains the result of an update policy operation (v2 API).
type UpdatePolicyResult struct {
	GuardID           string
	DenyPatternCount  int32
	AllowPatternCount int32
	CacheCleared      bool
}

// UpdatePolicy updates the allow/deny patterns of an existing guard (v2 API).
// This method uses the v2 proto which is only supported by Rust daemons.
func (m *GuardManager) UpdatePolicy(ctx context.Context, nodeIP, guardNamespace, guardName, podName string, denyPatterns, allowPatterns []string) (*UpdatePolicyResult, error) {
	logger := log.FromContext(ctx).WithValues(
		"guard", guardName,
		"namespace", guardNamespace,
		"pod", podName,
		"denyPatterns", len(denyPatterns),
		"allowPatterns", len(allowPatterns),
	)

	conn, err := m.client.GetConnection(ctx, nodeIP)
	if err != nil {
		return nil, fmt.Errorf("failed to get connection: %w", err)
	}

	logger.V(1).Info("Updating guard policy")

	// Create gRPC client and make the call
	client := januspb.NewJanusdServiceClient(conn)
	req := &januspb.UpdatePolicyRequest{
		GuardName:     guardName,
		Namespace:     guardNamespace,
		PodName:       podName,
		DenyPatterns:  denyPatterns,
		AllowPatterns: allowPatterns,
	}

	reqCtx, cancel := requestTimeout(ctx)
	defer cancel()

	resp, err := client.UpdatePolicy(reqCtx, req)
	if err != nil {
		return nil, fmt.Errorf("gRPC UpdatePolicy failed: %w", err)
	}

	logger.Info("Guard policy updated successfully",
		"guardID", resp.GuardId,
		"denyPatternCount", resp.DenyPatternCount,
		"allowPatternCount", resp.AllowPatternCount,
		"cacheCleared", resp.CacheCleared,
	)

	return &UpdatePolicyResult{
		GuardID:           resp.GuardId,
		DenyPatternCount:  resp.DenyPatternCount,
		AllowPatternCount: resp.AllowPatternCount,
		CacheCleared:      resp.CacheCleared,
	}, nil
}
