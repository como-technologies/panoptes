/*
Copyright 2026 Como Technologies, LTD.

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

// Package eventsink provides event export capabilities for Panoptes operators.
//
// It defines a common Event type and Sink interface that both the argus-operator
// (File Integrity Monitoring) and janus-operator (File Access Auditing) use to
// forward security events to external SIEMs such as Splunk, Elastic, and Datadog
// via HTTP webhooks.
package eventsink

import "context"

// Event represents a security event to be exported.
type Event struct {
	// Type identifies the kind of event, e.g. "file_modified", "access_denied", "access_audited".
	Type string `json:"type"`

	// Path is the filesystem path related to the event.
	Path string `json:"path"`

	// Timestamp is the time the event occurred in RFC 3339 format.
	Timestamp string `json:"timestamp"`

	// Namespace is the Kubernetes namespace where the event originated.
	Namespace string `json:"namespace"`

	// Pod is the name of the Kubernetes pod where the event originated.
	Pod string `json:"pod"`

	// Node is the name of the Kubernetes node where the event originated.
	Node string `json:"node"`

	// Tags holds optional key-value metadata for the event.
	Tags map[string]string `json:"tags,omitempty"`

	// Source identifies which Panoptes component generated the event ("argus" or "janus").
	Source string `json:"source"`

	// Severity indicates the importance of the event: "critical", "high", "medium", or "low".
	Severity string `json:"severity"`

	// Details holds optional additional information about the event.
	Details map[string]string `json:"details,omitempty"`
}

// Sink is the interface for event export destinations.
type Sink interface {
	// Send exports a single event to the destination.
	Send(ctx context.Context, event Event) error

	// SendBatch exports multiple events to the destination in a single operation.
	SendBatch(ctx context.Context, events []Event) error

	// Close flushes any pending events and releases resources.
	Close() error
}
