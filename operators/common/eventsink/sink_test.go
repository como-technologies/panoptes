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
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func sampleEvent() Event {
	return Event{
		Type:      "file_modified",
		Path:      "/etc/shadow",
		Timestamp: "2026-01-15T10:30:00Z",
		Namespace: "production",
		Pod:       "web-app-abc123",
		Node:      "worker-1",
		Source:    "argus",
		Severity:  "critical",
		Tags:      map[string]string{"env": "prod"},
		Details:   map[string]string{"hash_before": "abc", "hash_after": "def"},
	}
}

func TestNoopSink(t *testing.T) {
	sink := NewNoopSink()

	ctx := context.Background()
	if err := sink.Send(ctx, sampleEvent()); err != nil {
		t.Fatalf("NoopSink.Send returned error: %v", err)
	}
	if err := sink.SendBatch(ctx, []Event{sampleEvent(), sampleEvent()}); err != nil {
		t.Fatalf("NoopSink.SendBatch returned error: %v", err)
	}
	if err := sink.Close(); err != nil {
		t.Fatalf("NoopSink.Close returned error: %v", err)
	}
}

func TestNewWebhookSinkDisabled(t *testing.T) {
	sink := NewWebhookSink(Config{Enabled: false})

	// Should return a NoopSink when disabled.
	if _, ok := sink.(*NoopSink); !ok {
		t.Fatalf("expected NoopSink when Enabled=false, got %T", sink)
	}
	_ = sink.Close()
}

func TestWebhookSinkSend(t *testing.T) {
	var (
		mu       sync.Mutex
		received [][]Event
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if ct := r.Header.Get("Content-Type"); ct != "application/json" {
			t.Errorf("expected Content-Type application/json, got %s", ct)
		}
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Errorf("failed to read body: %v", err)
			return
		}
		var events []Event
		if err := json.Unmarshal(body, &events); err != nil {
			t.Errorf("failed to unmarshal events: %v", err)
			return
		}
		mu.Lock()
		received = append(received, events)
		mu.Unlock()
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     2,
		FlushInterval: 1 * time.Hour, // Large interval so only batch-size triggers flush.
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()

	// Send first event; should not trigger flush yet (batch size = 2).
	if err := sink.Send(ctx, sampleEvent()); err != nil {
		t.Fatalf("Send returned error: %v", err)
	}

	mu.Lock()
	count := len(received)
	mu.Unlock()
	if count != 0 {
		t.Fatalf("expected 0 batches after 1 event, got %d", count)
	}

	// Send second event; should trigger flush.
	if err := sink.Send(ctx, sampleEvent()); err != nil {
		t.Fatalf("Send returned error: %v", err)
	}

	mu.Lock()
	count = len(received)
	mu.Unlock()
	if count != 1 {
		t.Fatalf("expected 1 batch after 2 events, got %d", count)
	}

	mu.Lock()
	batch := received[0]
	mu.Unlock()
	if len(batch) != 2 {
		t.Fatalf("expected batch of 2 events, got %d", len(batch))
	}
	if batch[0].Type != "file_modified" {
		t.Errorf("expected event type file_modified, got %s", batch[0].Type)
	}

	if err := sink.Close(); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}
}

func TestWebhookSinkSendBatch(t *testing.T) {
	var (
		mu       sync.Mutex
		received [][]Event
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var events []Event
		_ = json.Unmarshal(body, &events)
		mu.Lock()
		received = append(received, events)
		mu.Unlock()
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     100,
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	events := []Event{sampleEvent(), sampleEvent(), sampleEvent()}

	if err := sink.SendBatch(ctx, events); err != nil {
		t.Fatalf("SendBatch returned error: %v", err)
	}

	mu.Lock()
	count := len(received)
	mu.Unlock()
	if count != 1 {
		t.Fatalf("expected 1 batch, got %d", count)
	}

	mu.Lock()
	batch := received[0]
	mu.Unlock()
	if len(batch) != 3 {
		t.Fatalf("expected 3 events in batch, got %d", len(batch))
	}

	_ = sink.Close()
}

func TestWebhookSinkSendBatchEmpty(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("server should not be called for empty batch")
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled: true,
		URL:     server.URL,
		Timeout: 5 * time.Second,
	})

	ctx := context.Background()
	if err := sink.SendBatch(ctx, []Event{}); err != nil {
		t.Fatalf("SendBatch with empty slice returned error: %v", err)
	}

	_ = sink.Close()
}

func TestWebhookSinkCustomHeaders(t *testing.T) {
	var headerReceived string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		headerReceived = r.Header.Get("Authorization")
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled: true,
		URL:     server.URL,
		Headers: map[string]string{
			"Authorization": "Bearer test-token-123",
		},
		BatchSize:     100,
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	if err := sink.SendBatch(ctx, []Event{sampleEvent()}); err != nil {
		t.Fatalf("SendBatch returned error: %v", err)
	}

	if headerReceived != "Bearer test-token-123" {
		t.Errorf("expected Authorization header 'Bearer test-token-123', got %q", headerReceived)
	}

	_ = sink.Close()
}

func TestWebhookSinkRetry(t *testing.T) {
	var attempts atomic.Int32

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		n := attempts.Add(1)
		if n < DefaultMaxRetries {
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     100,
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	if err := sink.SendBatch(ctx, []Event{sampleEvent()}); err != nil {
		t.Fatalf("SendBatch should have succeeded after retries: %v", err)
	}

	if got := attempts.Load(); got != int32(DefaultMaxRetries) {
		t.Errorf("expected %d attempts, got %d", DefaultMaxRetries, got)
	}

	_ = sink.Close()
}

func TestWebhookSinkRetryExhausted(t *testing.T) {
	var attempts atomic.Int32

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts.Add(1)
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     100,
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	err := sink.SendBatch(ctx, []Event{sampleEvent()})
	if err == nil {
		t.Fatal("SendBatch should have returned an error after exhausting retries")
	}

	if got := attempts.Load(); got != int32(DefaultMaxRetries) {
		t.Errorf("expected %d attempts, got %d", DefaultMaxRetries, got)
	}

	_ = sink.Close()
}

func TestWebhookSinkFlushOnClose(t *testing.T) {
	var (
		mu       sync.Mutex
		received [][]Event
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var events []Event
		_ = json.Unmarshal(body, &events)
		mu.Lock()
		received = append(received, events)
		mu.Unlock()
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     100, // Large batch size so events stay buffered.
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	_ = sink.Send(ctx, sampleEvent())

	// Close should flush the buffered event.
	if err := sink.Close(); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}

	mu.Lock()
	count := len(received)
	mu.Unlock()
	if count != 1 {
		t.Fatalf("expected 1 batch flushed on close, got %d", count)
	}

	mu.Lock()
	batch := received[0]
	mu.Unlock()
	if len(batch) != 1 {
		t.Fatalf("expected 1 event flushed on close, got %d", len(batch))
	}
}

func TestWebhookSinkSendAfterClose(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled: true,
		URL:     server.URL,
		Timeout: 5 * time.Second,
	})

	_ = sink.Close()

	ctx := context.Background()
	err := sink.Send(ctx, sampleEvent())
	if err == nil {
		t.Fatal("Send after Close should return an error")
	}
}

func TestWebhookSinkCloudEvents(t *testing.T) {
	var receivedBody []byte

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var err error
		receivedBody, err = io.ReadAll(r.Body)
		if err != nil {
			t.Errorf("failed to read body: %v", err)
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		CloudEvents:   true,
		BatchSize:     100,
		FlushInterval: 1 * time.Hour,
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	event := sampleEvent()
	if err := sink.SendBatch(ctx, []Event{event}); err != nil {
		t.Fatalf("SendBatch returned error: %v", err)
	}

	var cloudEvents []CloudEvent
	if err := json.Unmarshal(receivedBody, &cloudEvents); err != nil {
		t.Fatalf("failed to unmarshal CloudEvents: %v", err)
	}

	if len(cloudEvents) != 1 {
		t.Fatalf("expected 1 CloudEvent, got %d", len(cloudEvents))
	}

	ce := cloudEvents[0]
	if ce.SpecVersion != "1.0" {
		t.Errorf("expected specversion '1.0', got %q", ce.SpecVersion)
	}
	if ce.Type != "io.panoptes.event.file_modified" {
		t.Errorf("expected type 'io.panoptes.event.file_modified', got %q", ce.Type)
	}
	if ce.Source != "panoptes/argus" {
		t.Errorf("expected source 'panoptes/argus', got %q", ce.Source)
	}
	if ce.DataContentType != "application/json" {
		t.Errorf("expected datacontenttype 'application/json', got %q", ce.DataContentType)
	}
	if ce.Subject != "/etc/shadow" {
		t.Errorf("expected subject '/etc/shadow', got %q", ce.Subject)
	}
	if ce.ID == "" {
		t.Error("expected non-empty event ID")
	}
	if ce.Data.Type != "file_modified" {
		t.Errorf("expected data.type 'file_modified', got %q", ce.Data.Type)
	}

	_ = sink.Close()
}

func TestWebhookSinkFlushInterval(t *testing.T) {
	var (
		mu       sync.Mutex
		received [][]Event
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var events []Event
		_ = json.Unmarshal(body, &events)
		mu.Lock()
		received = append(received, events)
		mu.Unlock()
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	sink := NewWebhookSink(Config{
		Enabled:       true,
		URL:           server.URL,
		BatchSize:     100,           // Large batch size so it won't trigger.
		FlushInterval: 100 * time.Millisecond, // Short interval for testing.
		Timeout:       5 * time.Second,
	})

	ctx := context.Background()
	_ = sink.Send(ctx, sampleEvent())

	// Wait for the flush interval to fire.
	time.Sleep(300 * time.Millisecond)

	mu.Lock()
	count := len(received)
	mu.Unlock()
	if count < 1 {
		t.Fatalf("expected at least 1 batch after flush interval, got %d", count)
	}

	_ = sink.Close()
}

func TestCloudEventWrapping(t *testing.T) {
	event := Event{
		Type:      "access_denied",
		Path:      "/var/log/secure",
		Timestamp: "2026-01-15T10:30:00Z",
		Source:    "janus",
		Severity:  "high",
	}

	ce := WrapCloudEvent(event)

	if ce.SpecVersion != "1.0" {
		t.Errorf("expected specversion '1.0', got %q", ce.SpecVersion)
	}
	if ce.Type != "io.panoptes.event.access_denied" {
		t.Errorf("expected type 'io.panoptes.event.access_denied', got %q", ce.Type)
	}
	if ce.Source != "panoptes/janus" {
		t.Errorf("expected source 'panoptes/janus', got %q", ce.Source)
	}
	if ce.Time != "2026-01-15T10:30:00Z" {
		t.Errorf("expected time '2026-01-15T10:30:00Z', got %q", ce.Time)
	}
	if ce.Subject != "/var/log/secure" {
		t.Errorf("expected subject '/var/log/secure', got %q", ce.Subject)
	}
	if ce.ID == "" {
		t.Error("expected non-empty CloudEvent ID")
	}
}

func TestCloudEventWrappingEmptyTimestamp(t *testing.T) {
	event := Event{
		Type:   "file_modified",
		Source: "argus",
	}

	ce := WrapCloudEvent(event)

	if ce.Time == "" {
		t.Error("expected CloudEvent to have a generated timestamp when event timestamp is empty")
	}

	// Verify the generated timestamp is valid RFC3339.
	if _, err := time.Parse(time.RFC3339, ce.Time); err != nil {
		t.Errorf("generated timestamp is not valid RFC3339: %v", err)
	}
}

func TestCloudEventBatchWrapping(t *testing.T) {
	events := []Event{
		{Type: "file_modified", Source: "argus"},
		{Type: "access_denied", Source: "janus"},
	}

	wrapped := WrapCloudEvents(events)

	if len(wrapped) != 2 {
		t.Fatalf("expected 2 CloudEvents, got %d", len(wrapped))
	}

	if wrapped[0].Type != "io.panoptes.event.file_modified" {
		t.Errorf("first event type incorrect: %q", wrapped[0].Type)
	}
	if wrapped[1].Type != "io.panoptes.event.access_denied" {
		t.Errorf("second event type incorrect: %q", wrapped[1].Type)
	}

	// IDs should be unique.
	if wrapped[0].ID == wrapped[1].ID {
		t.Error("CloudEvent IDs should be unique")
	}
}

func TestConfigDefaults(t *testing.T) {
	cfg := Config{}.WithDefaults()

	if cfg.BatchSize != DefaultBatchSize {
		t.Errorf("expected BatchSize %d, got %d", DefaultBatchSize, cfg.BatchSize)
	}
	if cfg.FlushInterval != DefaultFlushInterval {
		t.Errorf("expected FlushInterval %v, got %v", DefaultFlushInterval, cfg.FlushInterval)
	}
	if cfg.Timeout != DefaultTimeout {
		t.Errorf("expected Timeout %v, got %v", DefaultTimeout, cfg.Timeout)
	}
}

func TestConfigWithExplicitValues(t *testing.T) {
	cfg := Config{
		BatchSize:     50,
		FlushInterval: 30 * time.Second,
		Timeout:       20 * time.Second,
	}.WithDefaults()

	if cfg.BatchSize != 50 {
		t.Errorf("expected BatchSize 50, got %d", cfg.BatchSize)
	}
	if cfg.FlushInterval != 30*time.Second {
		t.Errorf("expected FlushInterval 30s, got %v", cfg.FlushInterval)
	}
	if cfg.Timeout != 20*time.Second {
		t.Errorf("expected Timeout 20s, got %v", cfg.Timeout)
	}
}
