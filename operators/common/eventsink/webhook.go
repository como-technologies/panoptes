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
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"net/http"
	"sync"
	"time"
)

// WebhookSink exports events to an external system via HTTP POST webhooks.
// It supports batching, automatic flushing, and retry with exponential backoff.
type WebhookSink struct {
	config Config
	client *http.Client

	mu      sync.Mutex
	buffer  []Event
	timer   *time.Timer
	closed  bool
	closeCh chan struct{}
	doneCh  chan struct{}
}

// NewWebhookSink creates a new WebhookSink with the given configuration.
// If the configuration is not enabled, it returns a NoopSink instead.
func NewWebhookSink(config Config) Sink {
	if !config.Enabled {
		return NewNoopSink()
	}

	config = config.WithDefaults()

	ws := &WebhookSink{
		config: config,
		client: &http.Client{
			Timeout: config.Timeout,
		},
		buffer:  make([]Event, 0, config.BatchSize),
		closeCh: make(chan struct{}),
		doneCh:  make(chan struct{}),
	}

	// Start the flush timer goroutine.
	go ws.flushLoop()

	return ws
}

// Send adds a single event to the buffer and flushes if the batch size is reached.
func (ws *WebhookSink) Send(ctx context.Context, event Event) error {
	ws.mu.Lock()
	if ws.closed {
		ws.mu.Unlock()
		return fmt.Errorf("eventsink: sink is closed")
	}

	ws.buffer = append(ws.buffer, event)

	if len(ws.buffer) >= ws.config.BatchSize {
		batch := ws.drainBufferLocked()
		ws.mu.Unlock()
		return ws.sendBatch(ctx, batch)
	}

	ws.mu.Unlock()
	return nil
}

// SendBatch sends a batch of events immediately, bypassing the internal buffer.
func (ws *WebhookSink) SendBatch(ctx context.Context, events []Event) error {
	ws.mu.Lock()
	if ws.closed {
		ws.mu.Unlock()
		return fmt.Errorf("eventsink: sink is closed")
	}
	ws.mu.Unlock()

	if len(events) == 0 {
		return nil
	}

	return ws.sendBatch(ctx, events)
}

// Close flushes any remaining buffered events and releases resources.
func (ws *WebhookSink) Close() error {
	ws.mu.Lock()
	if ws.closed {
		ws.mu.Unlock()
		return nil
	}
	ws.closed = true
	batch := ws.drainBufferLocked()
	ws.mu.Unlock()

	// Signal the flush loop to stop.
	close(ws.closeCh)
	<-ws.doneCh

	// Flush remaining events.
	if len(batch) > 0 {
		ctx, cancel := context.WithTimeout(context.Background(), ws.config.Timeout)
		defer cancel()
		return ws.sendBatch(ctx, batch)
	}

	return nil
}

// flushLoop periodically flushes the buffer at the configured interval.
func (ws *WebhookSink) flushLoop() {
	defer close(ws.doneCh)

	ticker := time.NewTicker(ws.config.FlushInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ws.closeCh:
			return
		case <-ticker.C:
			ws.mu.Lock()
			if len(ws.buffer) == 0 {
				ws.mu.Unlock()
				continue
			}
			batch := ws.drainBufferLocked()
			ws.mu.Unlock()

			ctx, cancel := context.WithTimeout(context.Background(), ws.config.Timeout)
			// Ignore flush errors in the background loop; the events are best-effort.
			_ = ws.sendBatch(ctx, batch)
			cancel()
		}
	}
}

// drainBufferLocked removes and returns all events from the buffer.
// The caller must hold ws.mu.
func (ws *WebhookSink) drainBufferLocked() []Event {
	batch := ws.buffer
	ws.buffer = make([]Event, 0, ws.config.BatchSize)
	return batch
}

// sendBatch performs the HTTP POST with retry logic.
func (ws *WebhookSink) sendBatch(ctx context.Context, events []Event) error {
	var payload []byte
	var err error

	if ws.config.CloudEvents {
		wrapped := WrapCloudEvents(events)
		payload, err = json.Marshal(wrapped)
	} else {
		payload, err = json.Marshal(events)
	}
	if err != nil {
		return fmt.Errorf("eventsink: failed to marshal events: %w", err)
	}

	var lastErr error
	for attempt := range DefaultMaxRetries {
		if attempt > 0 {
			// Exponential backoff: 100ms, 200ms, 400ms, ...
			backoff := time.Duration(math.Pow(2, float64(attempt-1))) * 100 * time.Millisecond
			select {
			case <-ctx.Done():
				return fmt.Errorf("eventsink: context cancelled during retry: %w", ctx.Err())
			case <-time.After(backoff):
			}
		}

		lastErr = ws.doPost(ctx, payload)
		if lastErr == nil {
			return nil
		}
	}

	return fmt.Errorf("eventsink: failed after %d attempts: %w", DefaultMaxRetries, lastErr)
}

// doPost performs a single HTTP POST request.
func (ws *WebhookSink) doPost(ctx context.Context, payload []byte) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, ws.config.URL, bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("eventsink: failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	for key, value := range ws.config.Headers {
		req.Header.Set(key, value)
	}

	resp, err := ws.client.Do(req)
	if err != nil {
		return fmt.Errorf("eventsink: request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 200 && resp.StatusCode < 300 {
		return nil
	}

	return fmt.Errorf("eventsink: unexpected status code %d", resp.StatusCode)
}
