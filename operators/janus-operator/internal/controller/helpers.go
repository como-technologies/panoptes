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
	corev1 "k8s.io/api/core/v1"
)

// groupPodsByNode groups pods by their node name.
//
// Pods without a node assignment are excluded from the result.
// This is used to batch gRPC calls to daemons running on each node.
func groupPodsByNode(pods []corev1.Pod) map[string][]corev1.Pod {
	podsByNode := make(map[string][]corev1.Pod)
	for _, pod := range pods {
		if pod.Spec.NodeName != "" {
			podsByNode[pod.Spec.NodeName] = append(podsByNode[pod.Spec.NodeName], pod)
		}
	}
	return podsByNode
}

// isRunningPod checks if a pod is running with at least one running container.
//
// This filters out pods that are terminating, pending, or have no running
// containers, which wouldn't benefit from file monitoring.
func isRunningPod(pod *corev1.Pod) bool {
	if pod.Status.Phase != corev1.PodRunning || len(pod.Status.ContainerStatuses) == 0 {
		return false
	}
	for _, status := range pod.Status.ContainerStatuses {
		if status.State.Running != nil {
			return true
		}
	}
	return false
}

// filterRunningPods returns only pods that are running with at least one running container.
func filterRunningPods(pods []corev1.Pod) []corev1.Pod {
	var running []corev1.Pod
	for _, pod := range pods {
		if isRunningPod(&pod) {
			running = append(running, pod)
		}
	}
	return running
}

// pathListsEqual checks if two string slices contain the same paths in order.
func pathListsEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
