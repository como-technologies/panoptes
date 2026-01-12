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

	argusv1 "github.com/como-technologies/panoptes/operators/argus-operator/api/v1"
	"github.com/como-technologies/panoptes/operators/argus-operator/internal/daemon"
	"github.com/como-technologies/panoptes/operators/argus-operator/internal/metrics"
)

const (
	// FinalizerName is the finalizer for ArgusWatcher resources
	FinalizerName = "argus.como-technologies.io/finalizer"

	// DefaultRequeueAfter is the default requeue interval
	DefaultRequeueAfter = 60 * time.Second

	// ErrorRequeueAfter is the requeue interval after an error
	ErrorRequeueAfter = 30 * time.Second
)

// Condition types for ArgusWatcher
const (
	ConditionTypeAvailable   = "Available"
	ConditionTypeProgressing = "Progressing"
	ConditionTypeDegraded    = "Degraded"
)

// ArgusWatcherReconciler reconciles a ArgusWatcher object
type ArgusWatcherReconciler struct {
	client.Client
	Scheme       *runtime.Scheme
	Recorder     record.EventRecorder
	DaemonClient *daemon.Client
	WatchManager *daemon.WatchManager
}

// +kubebuilder:rbac:groups=argus.como-technologies.io,resources=arguswatchers,verbs=get;list;watch;create;update;patch;delete
// +kubebuilder:rbac:groups=argus.como-technologies.io,resources=arguswatchers/status,verbs=get;update;patch
// +kubebuilder:rbac:groups=argus.como-technologies.io,resources=arguswatchers/finalizers,verbs=update
// +kubebuilder:rbac:groups="",resources=pods,verbs=get;list;watch
// +kubebuilder:rbac:groups="",resources=nodes,verbs=get;list;watch
// +kubebuilder:rbac:groups="",resources=events,verbs=create;patch
// +kubebuilder:rbac:groups=coordination.k8s.io,resources=leases,verbs=get;list;watch;create;update;patch;delete

// Reconcile is the main reconciliation loop for ArgusWatcher resources.
func (r *ArgusWatcherReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	startTime := time.Now()
	logger := logf.FromContext(ctx)

	// Fetch the ArgusWatcher instance
	var watcher argusv1.ArgusWatcher
	if err := r.Get(ctx, req.NamespacedName, &watcher); err != nil {
		if apierrors.IsNotFound(err) {
			// Object was deleted, clean up metrics
			metrics.DeleteWatcherMetrics(req.Name, req.Namespace)
			return ctrl.Result{}, nil
		}
		logger.Error(err, "Failed to get ArgusWatcher")
		return ctrl.Result{}, err
	}

	// Record reconcile duration on exit
	defer func() {
		duration := time.Since(startTime).Seconds()
		result := "success"
		if r := recover(); r != nil {
			result = "panic"
			panic(r) // Re-panic after recording
		}
		metrics.RecordReconcile(watcher.Name, watcher.Namespace, result, duration)
	}()

	// Handle deletion
	if !watcher.DeletionTimestamp.IsZero() {
		return r.handleDeletion(ctx, &watcher)
	}

	// Add finalizer if not present
	if !controllerutil.ContainsFinalizer(&watcher, FinalizerName) {
		controllerutil.AddFinalizer(&watcher, FinalizerName)
		if err := r.Update(ctx, &watcher); err != nil {
			return ctrl.Result{}, err
		}
		return ctrl.Result{Requeue: true}, nil
	}

	// Check if paused
	if watcher.Spec.Paused {
		logger.Info("Watcher is paused, skipping reconciliation")
		r.setCondition(&watcher, ConditionTypeProgressing, metav1.ConditionFalse, "Paused", "Watcher is paused")
		if err := r.Status().Update(ctx, &watcher); err != nil {
			return ctrl.Result{}, err
		}
		return ctrl.Result{RequeueAfter: DefaultRequeueAfter}, nil
	}

	// Set progressing condition
	r.setCondition(&watcher, ConditionTypeProgressing, metav1.ConditionTrue, "Reconciling", "Reconciling watcher")

	// Find matching pods
	matchingPods, err := r.findMatchingPods(ctx, &watcher)
	if err != nil {
		logger.Error(err, "Failed to find matching pods")
		r.setCondition(&watcher, ConditionTypeDegraded, metav1.ConditionTrue, "PodListError", err.Error())
		if err := r.Status().Update(ctx, &watcher); err != nil {
			return ctrl.Result{}, err
		}
		return ctrl.Result{RequeueAfter: ErrorRequeueAfter}, err
	}

	// Update observable pods count
	watcher.Status.ObservablePods = int32(len(matchingPods))

	// Sync watches with daemon (daemon handles idempotency)
	watchedCount, watchDescriptors, podStatuses, err := r.syncWatches(ctx, &watcher, matchingPods)
	if err != nil {
		logger.Error(err, "Failed to sync watches")
		r.setCondition(&watcher, ConditionTypeDegraded, metav1.ConditionTrue, "SyncError", err.Error())
		r.Recorder.Eventf(&watcher, corev1.EventTypeWarning, "SyncFailed", "Failed to sync watches: %v", err)
	} else {
		r.setCondition(&watcher, ConditionTypeDegraded, metav1.ConditionFalse, "SyncSucceeded", "Watches synced successfully")
	}

	// Update status
	watcher.Status.WatchedPods = watchedCount
	watcher.Status.TotalWatchDescriptors = watchDescriptors
	watcher.Status.PodStatuses = podStatuses
	watcher.Status.ObservedGeneration = watcher.Generation
	now := metav1.Now()
	watcher.Status.LastReconcileTime = &now

	// Set available condition based on watched pods
	if watchedCount == watcher.Status.ObservablePods && watchedCount > 0 {
		r.setCondition(&watcher, ConditionTypeAvailable, metav1.ConditionTrue, "AllPodsWatched", "All matching pods are being watched")
	} else if watchedCount > 0 {
		r.setCondition(&watcher, ConditionTypeAvailable, metav1.ConditionTrue, "PartiallyWatched",
			fmt.Sprintf("Watching %d of %d matching pods", watchedCount, watcher.Status.ObservablePods))
	} else if watcher.Status.ObservablePods == 0 {
		r.setCondition(&watcher, ConditionTypeAvailable, metav1.ConditionFalse, "NoMatchingPods", "No pods match the selector")
	} else {
		r.setCondition(&watcher, ConditionTypeAvailable, metav1.ConditionFalse, "NoPodsWatched", "No pods are being watched")
	}

	r.setCondition(&watcher, ConditionTypeProgressing, metav1.ConditionFalse, "ReconcileComplete", "Reconciliation complete")

	// Update status
	if err := r.Status().Update(ctx, &watcher); err != nil {
		logger.Error(err, "Failed to update status")
		return ctrl.Result{}, err
	}

	// Update metrics
	metrics.UpdateWatcherMetrics(watcher.Name, watcher.Namespace, watcher.Status.ObservablePods, watcher.Status.WatchedPods, watcher.Status.TotalWatchDescriptors)

	logger.Info("Reconciliation complete",
		"observablePods", watcher.Status.ObservablePods,
		"watchedPods", watcher.Status.WatchedPods,
		"watchDescriptors", watcher.Status.TotalWatchDescriptors,
	)

	return ctrl.Result{RequeueAfter: DefaultRequeueAfter}, nil
}

// handleDeletion handles cleanup when the ArgusWatcher is being deleted.
func (r *ArgusWatcherReconciler) handleDeletion(ctx context.Context, watcher *argusv1.ArgusWatcher) (ctrl.Result, error) {
	logger := logf.FromContext(ctx)

	if controllerutil.ContainsFinalizer(watcher, FinalizerName) {
		logger.Info("Handling deletion, cleaning up watches")

		// Find all pods that might have watches
		matchingPods, err := r.findMatchingPods(ctx, watcher)
		if err != nil {
			logger.Error(err, "Failed to find matching pods during deletion")
			// Continue with deletion anyway
		}

		// Destroy watches on all matching pods
		for _, pod := range matchingPods {
			if pod.Spec.NodeName == "" {
				continue
			}

			// Get node IP
			var node corev1.Node
			if err := r.Get(ctx, types.NamespacedName{Name: pod.Spec.NodeName}, &node); err != nil {
				logger.Error(err, "Failed to get node", "node", pod.Spec.NodeName)
				continue
			}

			nodeIP := daemon.GetNodeIP(&node)
			if nodeIP == "" {
				continue
			}

			if err := r.WatchManager.DestroyWatch(ctx, nodeIP, watcher.Namespace, watcher.Name, pod.Name); err != nil {
				logger.Error(err, "Failed to destroy watch", "pod", pod.Name)
			}
		}

		// Remove finalizer
		controllerutil.RemoveFinalizer(watcher, FinalizerName)
		if err := r.Update(ctx, watcher); err != nil {
			return ctrl.Result{}, err
		}

		// Clean up metrics
		metrics.DeleteWatcherMetrics(watcher.Name, watcher.Namespace)

		r.Recorder.Event(watcher, corev1.EventTypeNormal, "Deleted", "ArgusWatcher deleted and watches cleaned up")
	}

	return ctrl.Result{}, nil
}

// findMatchingPods finds all pods that match the watcher's selector.
func (r *ArgusWatcherReconciler) findMatchingPods(ctx context.Context, watcher *argusv1.ArgusWatcher) ([]corev1.Pod, error) {
	selector, err := metav1.LabelSelectorAsSelector(&watcher.Spec.Selector)
	if err != nil {
		return nil, fmt.Errorf("invalid label selector: %w", err)
	}

	var podList corev1.PodList
	if err := r.List(ctx, &podList, &client.ListOptions{
		Namespace:     watcher.Namespace,
		LabelSelector: selector,
	}); err != nil {
		return nil, err
	}

	// Filter to only running pods with containers
	var runningPods []corev1.Pod
	for _, pod := range podList.Items {
		if pod.Status.Phase == corev1.PodRunning && len(pod.Status.ContainerStatuses) > 0 {
			// Check that at least one container is running
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

// syncWatches uses the query-first pattern to reconcile watches.
// It queries the daemon for actual state, compares with desired state,
// and only makes changes when needed.
func (r *ArgusWatcherReconciler) syncWatches(ctx context.Context, watcher *argusv1.ArgusWatcher, pods []corev1.Pod) (int32, int32, []argusv1.WatchedPodStatus, error) {
	logger := logf.FromContext(ctx)

	var watchedCount int32
	var totalWatchDescriptors int32
	var podStatuses []argusv1.WatchedPodStatus
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
		actualWatches, err := r.WatchManager.GetWatchState(ctx, nodeIP, watcher.Name, watcher.Namespace)
		if err != nil {
			logger.Error(err, "Failed to get watch state from daemon", "node", nodeName)
			// Fall back to creating all watches (daemon may be unavailable)
			actualWatches = nil
		}

		// Build map of actual watches by pod name
		actualByPod := make(map[string]daemon.WatchState)
		for _, w := range actualWatches {
			actualByPod[w.PodName] = w
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

			config := &daemon.WatchConfig{
				WatcherName:      watcher.Name,
				WatcherNamespace: watcher.Namespace,
				NodeName:         nodeName,
				NodeIP:           nodeIP,
				PodName:          pod.Name,
				PodNamespace:     pod.Namespace,
				ContainerIDs:     containerIDs,
				Subjects:         watcher.Spec.Subjects,
				LogFormat:        watcher.Spec.LogFormat,
				Paused:           watcher.Spec.Paused,
			}

			// Check if watch exists and config matches
			actual, exists := actualByPod[pod.Name]
			if exists && r.watchConfigMatches(watcher, &actual) {
				// Watch exists with matching config, skip creation
				logger.V(1).Info("Watch exists with matching config, skipping",
					"pod", pod.Name,
					"watchDescriptors", actual.WatchedPaths,
				)
				watchedCount++
				totalWatchDescriptors += actual.WatchedPaths
				podStatuses = append(podStatuses, argusv1.WatchedPodStatus{
					Name:             pod.Name,
					Namespace:        pod.Namespace,
					NodeName:         nodeName,
					WatchDescriptors: actual.WatchedPaths,
				})
				continue
			}

			// Create or update watch
			action := "Creating"
			if exists {
				action = "Updating"
			}
			logger.V(1).Info(action+" watch", "pod", pod.Name)

			result, err := r.WatchManager.CreateWatch(ctx, config)
			if err != nil {
				logger.Error(err, "Failed to create watch", "pod", pod.Name)
				lastErr = err
				continue
			}

			if result.Success {
				watchedCount++
				totalWatchDescriptors += result.WatchDescriptors
				podStatuses = append(podStatuses, argusv1.WatchedPodStatus{
					Name:             pod.Name,
					Namespace:        pod.Namespace,
					NodeName:         nodeName,
					WatchDescriptors: result.WatchDescriptors,
				})
			}
		}

		// 3. Destroy orphaned watches (exist in daemon but not in desired pods)
		for podName := range actualByPod {
			if !desiredPodNames[podName] {
				logger.Info("Destroying orphaned watch", "pod", podName, "node", nodeName)
				if err := r.WatchManager.DestroyWatch(ctx, nodeIP, watcher.Namespace, watcher.Name, podName); err != nil {
					logger.Error(err, "Failed to destroy orphaned watch", "pod", podName)
					// Don't set lastErr - this is cleanup, not critical
				}
			}
		}
	}

	return watchedCount, totalWatchDescriptors, podStatuses, lastErr
}

// watchConfigMatches checks if the daemon's actual watch config matches the desired spec.
func (r *ArgusWatcherReconciler) watchConfigMatches(watcher *argusv1.ArgusWatcher, actual *daemon.WatchState) bool {
	// Check paused state
	if actual.Paused != watcher.Spec.Paused {
		return false
	}

	// Check log format
	if actual.LogFormat != watcher.Spec.LogFormat {
		return false
	}

	// Check subjects count
	if len(actual.Subjects) != len(watcher.Spec.Subjects) {
		return false
	}

	// Check each subject
	for i, desired := range watcher.Spec.Subjects {
		if i >= len(actual.Subjects) {
			return false
		}
		actualSubj := actual.Subjects[i]

		// Compare paths
		if len(actualSubj.Paths) != len(desired.Paths) {
			return false
		}
		for j, p := range desired.Paths {
			if j >= len(actualSubj.Paths) || actualSubj.Paths[j] != p {
				return false
			}
		}

		// Compare recursive setting
		if actualSubj.Recursive != desired.Recursive {
			return false
		}

		// Compare max depth
		desiredMaxDepth := int32(0)
		if desired.MaxDepth != nil {
			desiredMaxDepth = *desired.MaxDepth
		}
		if actualSubj.MaxDepth != desiredMaxDepth {
			return false
		}
	}

	return true
}

// setCondition sets a condition on the ArgusWatcher status.
func (r *ArgusWatcherReconciler) setCondition(watcher *argusv1.ArgusWatcher, conditionType string, status metav1.ConditionStatus, reason, message string) {
	meta.SetStatusCondition(&watcher.Status.Conditions, metav1.Condition{
		Type:               conditionType,
		Status:             status,
		ObservedGeneration: watcher.Generation,
		Reason:             reason,
		Message:            message,
		LastTransitionTime: metav1.Now(),
	})

	// Update condition metric
	var value float64
	switch status {
	case metav1.ConditionTrue:
		value = 1
	case metav1.ConditionFalse:
		value = 0
	default:
		value = -1
	}
	metrics.WatcherCondition.WithLabelValues(watcher.Name, watcher.Namespace, conditionType).Set(value)
}

// SetupWithManager sets up the controller with the Manager.
func (r *ArgusWatcherReconciler) SetupWithManager(mgr ctrl.Manager) error {
	// Initialize daemon client if not set
	if r.DaemonClient == nil {
		r.DaemonClient = daemon.NewClient(0)
	}
	if r.WatchManager == nil {
		r.WatchManager = daemon.NewWatchManager(r.DaemonClient)
	}
	if r.Recorder == nil {
		r.Recorder = mgr.GetEventRecorderFor("arguswatcher-controller")
	}

	return ctrl.NewControllerManagedBy(mgr).
		For(&argusv1.ArgusWatcher{}, builder.WithPredicates(predicate.GenerationChangedPredicate{})).
		Watches(
			&corev1.Pod{},
			handler.EnqueueRequestsFromMapFunc(r.podToArgusWatcher),
		).
		Named("arguswatcher").
		Complete(r)
}

// podToArgusWatcher maps pod events to ArgusWatcher reconcile requests.
func (r *ArgusWatcherReconciler) podToArgusWatcher(ctx context.Context, obj client.Object) []reconcile.Request {
	pod, ok := obj.(*corev1.Pod)
	if !ok {
		return nil
	}

	// List all ArgusWatchers in the same namespace
	var watcherList argusv1.ArgusWatcherList
	if err := r.List(ctx, &watcherList, &client.ListOptions{
		Namespace: pod.Namespace,
	}); err != nil {
		return nil
	}

	var requests []reconcile.Request
	for _, watcher := range watcherList.Items {
		selector, err := metav1.LabelSelectorAsSelector(&watcher.Spec.Selector)
		if err != nil {
			continue
		}

		if selector.Matches(labels.Set(pod.Labels)) {
			requests = append(requests, reconcile.Request{
				NamespacedName: types.NamespacedName{
					Name:      watcher.Name,
					Namespace: watcher.Namespace,
				},
			})
		}
	}

	return requests
}
