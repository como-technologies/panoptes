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

package metrics

import (
	"github.com/prometheus/client_golang/prometheus"
	"sigs.k8s.io/controller-runtime/pkg/metrics"
)

const (
	namespace = "argus"
	subsystem = "controller"
)

var (
	// WatchersTotal tracks the total number of ArgusWatcher resources.
	WatchersTotal = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "watchers_total",
		Help:      "Total number of ArgusWatcher resources",
	})

	// WatchedPodsTotal tracks the total number of pods being watched.
	WatchedPodsTotal = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "watched_pods_total",
		Help:      "Total number of pods being watched",
	}, []string{"watcher", "namespace"})

	// ObservablePodsTotal tracks the total number of pods matching selectors.
	ObservablePodsTotal = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "observable_pods_total",
		Help:      "Total number of pods matching watcher selectors",
	}, []string{"watcher", "namespace"})

	// WatchDescriptorsTotal tracks the total inotify watch descriptors in use.
	WatchDescriptorsTotal = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "watch_descriptors_total",
		Help:      "Total inotify watch descriptors in use",
	}, []string{"watcher", "namespace"})

	// ReconcileTotal tracks the total number of reconciliations.
	ReconcileTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "reconcile_total",
		Help:      "Total number of reconciliations",
	}, []string{"watcher", "namespace", "result"})

	// ReconcileDuration tracks the duration of reconciliations.
	ReconcileDuration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "reconcile_duration_seconds",
		Help:      "Duration of reconciliations in seconds",
		Buckets:   prometheus.ExponentialBuckets(0.001, 2, 15), // 1ms to ~16s
	}, []string{"watcher", "namespace"})

	// DaemonConnectionsTotal tracks the number of daemon connections.
	DaemonConnectionsTotal = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "daemon_connections_total",
		Help:      "Total number of active daemon connections",
	})

	// DaemonRequestsTotal tracks the total number of daemon requests.
	DaemonRequestsTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "daemon_requests_total",
		Help:      "Total number of daemon gRPC requests",
	}, []string{"method", "result"})

	// DaemonRequestDuration tracks the duration of daemon requests.
	DaemonRequestDuration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "daemon_request_duration_seconds",
		Help:      "Duration of daemon gRPC requests in seconds",
		Buckets:   prometheus.ExponentialBuckets(0.001, 2, 12), // 1ms to ~2s
	}, []string{"method"})

	// EventsDetectedTotal tracks events detected across all watchers.
	EventsDetectedTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "events_detected_total",
		Help:      "Total file events detected",
	}, []string{"watcher", "namespace", "event_type"})

	// WatcherCondition tracks the condition status of watchers.
	WatcherCondition = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "watcher_condition",
		Help:      "Current condition status of watchers (1=True, 0=False, -1=Unknown)",
	}, []string{"watcher", "namespace", "condition"})
)

func init() {
	// Register all metrics with controller-runtime's registry
	metrics.Registry.MustRegister(
		WatchersTotal,
		WatchedPodsTotal,
		ObservablePodsTotal,
		WatchDescriptorsTotal,
		ReconcileTotal,
		ReconcileDuration,
		DaemonConnectionsTotal,
		DaemonRequestsTotal,
		DaemonRequestDuration,
		EventsDetectedTotal,
		WatcherCondition,
	)
}

// RecordReconcile records metrics for a reconciliation.
func RecordReconcile(watcher, namespace, result string, duration float64) {
	ReconcileTotal.WithLabelValues(watcher, namespace, result).Inc()
	ReconcileDuration.WithLabelValues(watcher, namespace).Observe(duration)
}

// RecordDaemonRequest records metrics for a daemon request.
func RecordDaemonRequest(method, result string, duration float64) {
	DaemonRequestsTotal.WithLabelValues(method, result).Inc()
	DaemonRequestDuration.WithLabelValues(method).Observe(duration)
}

// UpdateWatcherMetrics updates metrics for a specific watcher.
func UpdateWatcherMetrics(watcher, namespace string, observable, watched, watchDescriptors int32) {
	ObservablePodsTotal.WithLabelValues(watcher, namespace).Set(float64(observable))
	WatchedPodsTotal.WithLabelValues(watcher, namespace).Set(float64(watched))
	WatchDescriptorsTotal.WithLabelValues(watcher, namespace).Set(float64(watchDescriptors))
}

// DeleteWatcherMetrics removes metrics for a deleted watcher.
func DeleteWatcherMetrics(watcher, namespace string) {
	ObservablePodsTotal.DeleteLabelValues(watcher, namespace)
	WatchedPodsTotal.DeleteLabelValues(watcher, namespace)
	WatchDescriptorsTotal.DeleteLabelValues(watcher, namespace)
	ReconcileTotal.DeletePartialMatch(prometheus.Labels{"watcher": watcher, "namespace": namespace})
	ReconcileDuration.DeleteLabelValues(watcher, namespace)
	WatcherCondition.DeletePartialMatch(prometheus.Labels{"watcher": watcher, "namespace": namespace})
}
