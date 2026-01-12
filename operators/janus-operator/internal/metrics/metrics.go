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
	namespace = "janus"
	subsystem = "controller"
)

var (
	// GuardsTotal tracks the total number of JanusGuard resources.
	GuardsTotal = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "guards_total",
		Help:      "Total number of JanusGuard resources",
	})

	// GuardedPodsTotal tracks the total number of pods being guarded.
	GuardedPodsTotal = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "guarded_pods_total",
		Help:      "Total number of pods being guarded",
	}, []string{"guard", "namespace"})

	// ObservablePodsTotal tracks the total number of pods matching selectors.
	ObservablePodsTotal = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "observable_pods_total",
		Help:      "Total number of pods matching guard selectors",
	}, []string{"guard", "namespace"})

	// DeniedAccessTotal tracks the total number of denied accesses.
	DeniedAccessTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "denied_access_total",
		Help:      "Total number of denied file accesses",
	}, []string{"guard", "namespace"})

	// AllowedAccessTotal tracks the total number of allowed accesses.
	AllowedAccessTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "allowed_access_total",
		Help:      "Total number of allowed file accesses",
	}, []string{"guard", "namespace"})

	// AuditedAccessTotal tracks the total number of audited accesses.
	AuditedAccessTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "audited_access_total",
		Help:      "Total number of audited file accesses",
	}, []string{"guard", "namespace"})

	// ReconcileTotal tracks the total number of reconciliations.
	ReconcileTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "reconcile_total",
		Help:      "Total number of reconciliations",
	}, []string{"guard", "namespace", "result"})

	// ReconcileDuration tracks the duration of reconciliations.
	ReconcileDuration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "reconcile_duration_seconds",
		Help:      "Duration of reconciliations in seconds",
		Buckets:   prometheus.ExponentialBuckets(0.001, 2, 15),
	}, []string{"guard", "namespace"})

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
		Buckets:   prometheus.ExponentialBuckets(0.001, 2, 12),
	}, []string{"method"})

	// GuardCondition tracks the condition status of guards.
	GuardCondition = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: namespace,
		Subsystem: subsystem,
		Name:      "guard_condition",
		Help:      "Current condition status of guards (1=True, 0=False, -1=Unknown)",
	}, []string{"guard", "namespace", "condition"})
)

func init() {
	metrics.Registry.MustRegister(
		GuardsTotal,
		GuardedPodsTotal,
		ObservablePodsTotal,
		DeniedAccessTotal,
		AllowedAccessTotal,
		AuditedAccessTotal,
		ReconcileTotal,
		ReconcileDuration,
		DaemonConnectionsTotal,
		DaemonRequestsTotal,
		DaemonRequestDuration,
		GuardCondition,
	)
}

// RecordReconcile records metrics for a reconciliation.
func RecordReconcile(guard, namespace, result string, duration float64) {
	ReconcileTotal.WithLabelValues(guard, namespace, result).Inc()
	ReconcileDuration.WithLabelValues(guard, namespace).Observe(duration)
}

// RecordDaemonRequest records metrics for a daemon request.
func RecordDaemonRequest(method, result string, duration float64) {
	DaemonRequestsTotal.WithLabelValues(method, result).Inc()
	DaemonRequestDuration.WithLabelValues(method).Observe(duration)
}

// UpdateGuardMetrics updates metrics for a specific guard.
func UpdateGuardMetrics(guard, namespace string, observable, guarded int32) {
	ObservablePodsTotal.WithLabelValues(guard, namespace).Set(float64(observable))
	GuardedPodsTotal.WithLabelValues(guard, namespace).Set(float64(guarded))
}

// DeleteGuardMetrics removes metrics for a deleted guard.
func DeleteGuardMetrics(guard, namespace string) {
	ObservablePodsTotal.DeleteLabelValues(guard, namespace)
	GuardedPodsTotal.DeleteLabelValues(guard, namespace)
	ReconcileTotal.DeletePartialMatch(prometheus.Labels{"guard": guard, "namespace": namespace})
	ReconcileDuration.DeleteLabelValues(guard, namespace)
	GuardCondition.DeletePartialMatch(prometheus.Labels{"guard": guard, "namespace": namespace})
}
