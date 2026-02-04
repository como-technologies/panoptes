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

package controller

import (
	"context"
	"fmt"
	"time"

	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	"k8s.io/apimachinery/pkg/api/meta"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/labels"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/client-go/tools/record"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/builder"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"
	"sigs.k8s.io/controller-runtime/pkg/handler"
	logf "sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/controller-runtime/pkg/predicate"
	"sigs.k8s.io/controller-runtime/pkg/reconcile"

	janusv2 "github.com/como-technologies/panoptes/operators/janus-operator/api/v2"
	"github.com/como-technologies/panoptes/operators/janus-operator/internal/daemon"
	"github.com/como-technologies/panoptes/operators/janus-operator/internal/metrics"
)

const (
	// FinalizerName is the finalizer for JanusGuard resources
	FinalizerName = "janus.como-technologies.io/finalizer"

	// DefaultRequeueAfter is the default requeue interval
	DefaultRequeueAfter = 60 * time.Second

	// ErrorRequeueAfter is the requeue interval after an error
	ErrorRequeueAfter = 30 * time.Second
)

// Condition types for JanusGuard
const (
	ConditionTypeAvailable   = "Available"
	ConditionTypeProgressing = "Progressing"
	ConditionTypeDegraded    = "Degraded"
	ConditionTypeStalled     = "Stalled"
)

// JanusGuardReconciler reconciles a JanusGuard object
type JanusGuardReconciler struct {
	client.Client
	Scheme       *runtime.Scheme
	Recorder     record.EventRecorder
	DaemonClient *daemon.Client
	GuardManager *daemon.GuardManager
}

// +kubebuilder:rbac:groups=janus.como-technologies.io,resources=janusguards,verbs=get;list;watch;create;update;patch;delete
// +kubebuilder:rbac:groups=janus.como-technologies.io,resources=janusguards/status,verbs=get;update;patch
// +kubebuilder:rbac:groups=janus.como-technologies.io,resources=janusguards/finalizers,verbs=update
// +kubebuilder:rbac:groups="",resources=pods,verbs=get;list;watch
// +kubebuilder:rbac:groups="",resources=nodes,verbs=get;list;watch
// +kubebuilder:rbac:groups="",resources=events,verbs=create;patch
// +kubebuilder:rbac:groups=coordination.k8s.io,resources=leases,verbs=get;list;watch;create;update;patch;delete

// Reconcile is the main reconciliation loop for JanusGuard resources.
func (r *JanusGuardReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	startTime := time.Now()
	logger := logf.FromContext(ctx)

	// Fetch the JanusGuard instance
	var guard janusv2.JanusGuard
	if err := r.Get(ctx, req.NamespacedName, &guard); err != nil {
		if apierrors.IsNotFound(err) {
			metrics.DeleteGuardMetrics(req.Name, req.Namespace)
			return ctrl.Result{}, nil
		}
		logger.Error(err, "Failed to get JanusGuard")
		return ctrl.Result{}, err
	}

	// Record reconcile duration on exit
	defer func() {
		duration := time.Since(startTime).Seconds()
		result := "success"
		if r := recover(); r != nil {
			result = "panic"
			panic(r)
		}
		metrics.RecordReconcile(guard.Name, guard.Namespace, result, duration)
	}()

	// Handle deletion
	if !guard.DeletionTimestamp.IsZero() {
		return r.handleDeletion(ctx, &guard)
	}

	// Add finalizer if not present
	if !controllerutil.ContainsFinalizer(&guard, FinalizerName) {
		controllerutil.AddFinalizer(&guard, FinalizerName)
		if err := r.Update(ctx, &guard); err != nil {
			return ctrl.Result{}, err
		}
		return ctrl.Result{Requeue: true}, nil
	}

	// Set progressing condition (note: we reconcile even when paused to pass config to daemon)
	r.setCondition(&guard, ConditionTypeProgressing, metav1.ConditionTrue, "Reconciling", "Reconciling guard")

	// Find matching pods
	matchingPods, err := r.findMatchingPods(ctx, &guard)
	if err != nil {
		logger.Error(err, "Failed to find matching pods")
		r.setCondition(&guard, ConditionTypeDegraded, metav1.ConditionTrue, "PodListError", err.Error())
		if err := r.Status().Update(ctx, &guard); err != nil {
			return ctrl.Result{}, err
		}
		return ctrl.Result{RequeueAfter: ErrorRequeueAfter}, err
	}

	// Update observable pods count
	guard.Status.ObservablePods = int32(len(matchingPods))

	// Sync guards with daemon (daemon handles idempotency)
	guardedCount, podStatuses, err := r.syncGuards(ctx, &guard, matchingPods)
	if err != nil {
		logger.Error(err, "Failed to sync guards")
		r.setCondition(&guard, ConditionTypeDegraded, metav1.ConditionTrue, "SyncError", err.Error())
		r.Recorder.Eventf(&guard, corev1.EventTypeWarning, "SyncFailed", "Failed to sync guards: %v", err)
	} else {
		r.setCondition(&guard, ConditionTypeDegraded, metav1.ConditionFalse, "SyncSucceeded", "Guards synced successfully")
	}

	// Update status
	guard.Status.GuardedPods = guardedCount
	guard.Status.PodStatuses = podStatuses
	guard.Status.ObservedGeneration = guard.Generation
	now := metav1.Now()
	guard.Status.LastReconcileTime = &now

	// Set available condition based on guarded pods
	if guardedCount == guard.Status.ObservablePods && guardedCount > 0 {
		r.setCondition(&guard, ConditionTypeAvailable, metav1.ConditionTrue, "AllPodsGuarded", "All matching pods are being guarded")
	} else if guardedCount > 0 {
		r.setCondition(&guard, ConditionTypeAvailable, metav1.ConditionTrue, "PartiallyGuarded",
			fmt.Sprintf("Guarding %d of %d matching pods", guardedCount, guard.Status.ObservablePods))
	} else if guard.Status.ObservablePods == 0 {
		r.setCondition(&guard, ConditionTypeAvailable, metav1.ConditionFalse, "NoMatchingPods", "No pods match the selector")
	} else {
		r.setCondition(&guard, ConditionTypeAvailable, metav1.ConditionFalse, "NoPodsGuarded", "No pods are being guarded")
	}

	// Detect stalled state: observable pods exist but none can be guarded
	if guard.Status.ObservablePods > 0 && guardedCount == 0 && err != nil {
		r.setCondition(&guard, ConditionTypeStalled, metav1.ConditionTrue, "NoDaemonReachable",
			"No daemon pods are reachable; guards cannot be established")
		r.Recorder.Event(&guard, corev1.EventTypeWarning, "Stalled", "Unable to reach any daemon pods for guard creation")
	} else {
		r.setCondition(&guard, ConditionTypeStalled, metav1.ConditionFalse, "Operational", "Controller is operating normally")
	}

	r.setCondition(&guard, ConditionTypeProgressing, metav1.ConditionFalse, "ReconcileComplete", "Reconciliation complete")

	// Update status
	if err := r.Status().Update(ctx, &guard); err != nil {
		logger.Error(err, "Failed to update status")
		return ctrl.Result{}, err
	}

	// Update metrics
	metrics.UpdateGuardMetrics(guard.Name, guard.Namespace, guard.Status.ObservablePods, guard.Status.GuardedPods)

	logger.Info("Reconciliation complete",
		"observablePods", guard.Status.ObservablePods,
		"guardedPods", guard.Status.GuardedPods,
		"paused", guard.Spec.Paused,
		"enforcing", guard.Spec.Enforcing,
	)

	return ctrl.Result{RequeueAfter: DefaultRequeueAfter}, nil
}

// handleDeletion handles cleanup when the JanusGuard is being deleted.
//
//nolint:unparam // Result is always zero but signature matches reconciler pattern
func (r *JanusGuardReconciler) handleDeletion(ctx context.Context, guard *janusv2.JanusGuard) (ctrl.Result, error) {
	logger := logf.FromContext(ctx)

	if controllerutil.ContainsFinalizer(guard, FinalizerName) {
		logger.Info("Handling deletion, cleaning up guards")

		matchingPods, err := r.findMatchingPods(ctx, guard)
		if err != nil {
			logger.Error(err, "Failed to find matching pods during deletion")
		}

		// Destroy guards on all matching pods
		for _, pod := range matchingPods {
			if pod.Spec.NodeName == "" {
				continue
			}

			var node corev1.Node
			if err := r.Get(ctx, types.NamespacedName{Name: pod.Spec.NodeName}, &node); err != nil {
				logger.Error(err, "Failed to get node", "node", pod.Spec.NodeName)
				continue
			}

			nodeIP := daemon.GetNodeIP(&node)
			if nodeIP == "" {
				continue
			}

			if err := r.GuardManager.DestroyGuard(ctx, nodeIP, guard.Namespace, guard.Name, pod.Name); err != nil {
				logger.Error(err, "Failed to destroy guard", "pod", pod.Name)
			}
		}

		controllerutil.RemoveFinalizer(guard, FinalizerName)
		if err := r.Update(ctx, guard); err != nil {
			return ctrl.Result{}, err
		}

		metrics.DeleteGuardMetrics(guard.Name, guard.Namespace)
		r.Recorder.Event(guard, corev1.EventTypeNormal, "Deleted", "JanusGuard deleted and guards cleaned up")
	}

	return ctrl.Result{}, nil
}

// findMatchingPods finds all pods that match the guard's selector.
func (r *JanusGuardReconciler) findMatchingPods(ctx context.Context, guard *janusv2.JanusGuard) ([]corev1.Pod, error) {
	selector, err := metav1.LabelSelectorAsSelector(&guard.Spec.Selector)
	if err != nil {
		return nil, fmt.Errorf("invalid label selector: %w", err)
	}

	var podList corev1.PodList
	if err := r.List(ctx, &podList, &client.ListOptions{
		Namespace:     guard.Namespace,
		LabelSelector: selector,
	}); err != nil {
		return nil, err
	}

	var runningPods []corev1.Pod
	for _, pod := range podList.Items {
		if pod.Status.Phase == corev1.PodRunning && len(pod.Status.ContainerStatuses) > 0 {
			hasRunning := false
			for _, status := range pod.Status.ContainerStatuses {
				if status.State.Running != nil {
					hasRunning = true
					break
				}
			}
			if hasRunning {
				runningPods = append(runningPods, pod)
			}
		}
	}

	return runningPods, nil
}

// syncGuards uses the query-first pattern to reconcile guards.
// It queries the daemon for actual state, compares with desired state,
// and only makes changes when needed.
func (r *JanusGuardReconciler) syncGuards(ctx context.Context, guard *janusv2.JanusGuard, pods []corev1.Pod) (int32, []janusv2.GuardedPodStatus, error) {
	logger := logf.FromContext(ctx)

	var guardedCount int32
	var podStatuses []janusv2.GuardedPodStatus
	var lastErr error

	// Group pods by node
	podsByNode := make(map[string][]corev1.Pod)
	for _, pod := range pods {
		if pod.Spec.NodeName != "" {
			podsByNode[pod.Spec.NodeName] = append(podsByNode[pod.Spec.NodeName], pod)
		}
	}

	// Node cache for IP lookups
	nodeCache := make(map[string]*corev1.Node)

	for nodeName, nodePods := range podsByNode {
		// Get node info
		node, ok := nodeCache[nodeName]
		if !ok {
			var n corev1.Node
			if err := r.Get(ctx, types.NamespacedName{Name: nodeName}, &n); err != nil {
				logger.Error(err, "Failed to get node", "node", nodeName)
				lastErr = err
				continue
			}
			node = &n
			nodeCache[nodeName] = node
		}

		nodeIP := daemon.GetNodeIP(node)
		if nodeIP == "" {
			logger.V(1).Info("Node has no internal IP", "node", nodeName)
			continue
		}

		// 1. Query actual state from daemon
		actualGuards, err := r.GuardManager.GetGuardState(ctx, nodeIP, guard.Name, guard.Namespace)
		if err != nil {
			logger.Error(err, "Failed to get guard state from daemon", "node", nodeName)
			// Fall back to creating all guards (daemon may be unavailable)
			actualGuards = nil
		}

		// Build map of actual guards by pod name
		actualByPod := make(map[string]daemon.GuardState)
		for _, g := range actualGuards {
			actualByPod[g.PodName] = g
		}

		// 2. Build desired state and reconcile
		desiredPodNames := make(map[string]bool)
		for _, pod := range nodePods {
			desiredPodNames[pod.Name] = true

			containerIDs := daemon.GetContainerIDs(&pod)
			if len(containerIDs) == 0 {
				logger.V(1).Info("Pod has no container IDs", "pod", pod.Name)
				continue
			}

			config := &daemon.GuardConfig{
				GuardName:      guard.Name,
				GuardNamespace: guard.Namespace,
				NodeName:       nodeName,
				NodeIP:         nodeIP,
				PodName:        pod.Name,
				PodNamespace:   pod.Namespace,
				ContainerIDs:   containerIDs,
				Subjects:       guard.Spec.Subjects,
				LogFormat:      guard.Spec.LogFormat,
				Paused:         guard.Spec.Paused,
				Enforcing:      guard.Spec.Enforcing,
			}

			// Check if guard exists and config matches
			actual, exists := actualByPod[pod.Name]
			if exists && r.guardConfigMatches(guard, &actual) {
				// Guard exists with matching config, skip creation
				logger.V(1).Info("Guard exists with matching config, skipping",
					"pod", pod.Name,
					"guardedPaths", actual.GuardedPaths,
				)
				guardedCount++
				podStatuses = append(podStatuses, janusv2.GuardedPodStatus{
					Name:         pod.Name,
					Namespace:    pod.Namespace,
					NodeName:     nodeName,
					DeniedCount:  0,
					AllowedCount: 0,
				})
				continue
			}

			// Create or update guard
			action := "Creating"
			if exists {
				action = "Updating"
			}
			logger.V(1).Info(action+" guard", "pod", pod.Name)

			result, err := r.GuardManager.CreateGuard(ctx, config)
			if err != nil {
				logger.Error(err, "Failed to create guard", "pod", pod.Name)
				lastErr = err
				continue
			}

			if result.Success {
				guardedCount++
				podStatuses = append(podStatuses, janusv2.GuardedPodStatus{
					Name:         pod.Name,
					Namespace:    pod.Namespace,
					NodeName:     nodeName,
					DeniedCount:  0,
					AllowedCount: 0,
				})
			}
		}

		// 3. Destroy orphaned guards (exist in daemon but not in desired pods)
		for podName := range actualByPod {
			if !desiredPodNames[podName] {
				logger.Info("Destroying orphaned guard", "pod", podName, "node", nodeName)
				if err := r.GuardManager.DestroyGuard(ctx, nodeIP, guard.Namespace, guard.Name, podName); err != nil {
					logger.Error(err, "Failed to destroy orphaned guard", "pod", podName)
					// Don't set lastErr - this is cleanup, not critical
				}
			}
		}
	}

	return guardedCount, podStatuses, lastErr
}

// guardConfigMatches checks if the daemon's actual guard config matches the desired spec.
func (r *JanusGuardReconciler) guardConfigMatches(guard *janusv2.JanusGuard, actual *daemon.GuardState) bool {
	// Check paused state
	if actual.Paused != guard.Spec.Paused {
		return false
	}

	// Check enforcing state
	if actual.Enforcing != guard.Spec.Enforcing {
		return false
	}

	// Check log format
	if actual.LogFormat != guard.Spec.LogFormat {
		return false
	}

	// Check subjects count
	if len(actual.Subjects) != len(guard.Spec.Subjects) {
		return false
	}

	// Check each subject
	for i, desired := range guard.Spec.Subjects {
		if i >= len(actual.Subjects) {
			return false
		}
		actualSubj := actual.Subjects[i]

		// Compare allow paths
		if len(actualSubj.Allow) != len(desired.Allow) {
			return false
		}
		for j, p := range desired.Allow {
			if j >= len(actualSubj.Allow) || actualSubj.Allow[j] != p {
				return false
			}
		}

		// Compare deny paths
		if len(actualSubj.Deny) != len(desired.Deny) {
			return false
		}
		for j, p := range desired.Deny {
			if j >= len(actualSubj.Deny) || actualSubj.Deny[j] != p {
				return false
			}
		}
	}

	return true
}

// setCondition sets a condition on the JanusGuard status.
// Note: We do NOT set LastTransitionTime explicitly - meta.SetStatusCondition()
// will preserve the existing timestamp if the condition status hasn't changed,
// preventing unnecessary reconciliation loops.
func (r *JanusGuardReconciler) setCondition(guard *janusv2.JanusGuard, conditionType string, status metav1.ConditionStatus, reason, message string) {
	meta.SetStatusCondition(&guard.Status.Conditions, metav1.Condition{
		Type:               conditionType,
		Status:             status,
		ObservedGeneration: guard.Generation,
		Reason:             reason,
		Message:            message,
	})

	var value float64
	switch status {
	case metav1.ConditionTrue:
		value = 1
	case metav1.ConditionFalse:
		value = 0
	default:
		value = -1
	}
	metrics.GuardCondition.WithLabelValues(guard.Name, guard.Namespace, conditionType).Set(value)
}

// SetupWithManager sets up the controller with the Manager.
func (r *JanusGuardReconciler) SetupWithManager(mgr ctrl.Manager) error {
	if r.DaemonClient == nil {
		r.DaemonClient = daemon.NewClient(0)
	}
	if r.GuardManager == nil {
		r.GuardManager = daemon.NewGuardManager(r.DaemonClient)
	}
	if r.Recorder == nil {
		r.Recorder = mgr.GetEventRecorderFor("janusguard-controller")
	}

	return ctrl.NewControllerManagedBy(mgr).
		For(&janusv2.JanusGuard{}, builder.WithPredicates(predicate.GenerationChangedPredicate{})).
		Watches(
			&corev1.Pod{},
			handler.EnqueueRequestsFromMapFunc(r.podToJanusGuard),
		).
		Named("janusguard").
		Complete(r)
}

// podToJanusGuard maps pod events to JanusGuard reconcile requests.
func (r *JanusGuardReconciler) podToJanusGuard(ctx context.Context, obj client.Object) []reconcile.Request {
	pod, ok := obj.(*corev1.Pod)
	if !ok {
		return nil
	}

	var guardList janusv2.JanusGuardList
	if err := r.List(ctx, &guardList, &client.ListOptions{
		Namespace: pod.Namespace,
	}); err != nil {
		return nil
	}

	var requests []reconcile.Request
	for _, guard := range guardList.Items {
		selector, err := metav1.LabelSelectorAsSelector(&guard.Spec.Selector)
		if err != nil {
			continue
		}

		if selector.Matches(labels.Set(pod.Labels)) {
			requests = append(requests, reconcile.Request{
				NamespacedName: types.NamespacedName{
					Name:      guard.Name,
					Namespace: guard.Namespace,
				},
			})
		}
	}

	return requests
}
