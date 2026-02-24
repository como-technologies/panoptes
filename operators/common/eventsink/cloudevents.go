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

package eventsink

import (
	"fmt"
	"time"

	"github.com/google/uuid"
)

const (
	// cloudEventsSpecVersion is the CloudEvents specification version.
	cloudEventsSpecVersion = "1.0"

	// cloudEventsTypePrefix is the prefix for CloudEvents type attributes.
	cloudEventsTypePrefix = "io.panoptes.event"

	// cloudEventsSourcePrefix is the prefix for CloudEvents source attributes.
	cloudEventsSourcePrefix = "panoptes"
)

// CloudEvent represents an event wrapped in the CloudEvents v1.0 specification.
// See https://cloudevents.io/ for the full specification.
type CloudEvent struct {
	// SpecVersion is the CloudEvents specification version (always "1.0").
	SpecVersion string `json:"specversion"`

	// ID is a unique identifier for this event.
	ID string `json:"id"`

	// Type identifies the type of event, e.g. "io.panoptes.event.file_modified".
	Type string `json:"type"`

	// Source identifies the context in which the event happened, e.g. "panoptes/argus".
	Source string `json:"source"`

	// Time is the timestamp of when the event occurred in RFC 3339 format.
	Time string `json:"time"`

	// DataContentType is the content type of the data attribute (always "application/json").
	DataContentType string `json:"datacontenttype"`

	// Subject is an optional subject of the event, set to the file path.
	Subject string `json:"subject,omitempty"`

	// Data contains the actual event payload.
	Data Event `json:"data"`
}

// WrapCloudEvent wraps an Event in a CloudEvents v1.0 envelope.
func WrapCloudEvent(event Event) CloudEvent {
	eventTime := event.Timestamp
	if eventTime == "" {
		eventTime = time.Now().UTC().Format(time.RFC3339)
	}

	return CloudEvent{
		SpecVersion:     cloudEventsSpecVersion,
		ID:              uuid.New().String(),
		Type:            fmt.Sprintf("%s.%s", cloudEventsTypePrefix, event.Type),
		Source:          fmt.Sprintf("%s/%s", cloudEventsSourcePrefix, event.Source),
		Time:            eventTime,
		DataContentType: "application/json",
		Subject:         event.Path,
		Data:            event,
	}
}

// WrapCloudEvents wraps a slice of Events in CloudEvents v1.0 envelopes.
func WrapCloudEvents(events []Event) []CloudEvent {
	wrapped := make([]CloudEvent, len(events))
	for i, event := range events {
		wrapped[i] = WrapCloudEvent(event)
	}
	return wrapped
}
