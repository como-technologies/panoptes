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

import "context"

// NoopSink is a Sink implementation that silently discards all events.
// It is used when event export is disabled.
type NoopSink struct{}

// NewNoopSink returns a new NoopSink.
func NewNoopSink() *NoopSink {
	return &NoopSink{}
}

// Send discards the event and returns nil.
func (n *NoopSink) Send(_ context.Context, _ Event) error {
	return nil
}

// SendBatch discards all events and returns nil.
func (n *NoopSink) SendBatch(_ context.Context, _ []Event) error {
	return nil
}

// Close is a no-op and returns nil.
func (n *NoopSink) Close() error {
	return nil
}
