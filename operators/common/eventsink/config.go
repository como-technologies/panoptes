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

import "time"

const (
	// DefaultBatchSize is the default number of events to accumulate before flushing.
	DefaultBatchSize = 100

	// DefaultFlushInterval is the default maximum time to wait before flushing a partial batch.
	DefaultFlushInterval = 5 * time.Second

	// DefaultTimeout is the default HTTP request timeout for webhook calls.
	DefaultTimeout = 10 * time.Second

	// DefaultMaxRetries is the default number of retry attempts for failed webhook calls.
	DefaultMaxRetries = 3
)

// Config holds configuration for an event sink.
type Config struct {
	// Enabled controls whether event export is active. When false, a no-op sink is used.
	Enabled bool

	// URL is the HTTP endpoint to POST events to.
	URL string

	// Headers contains additional HTTP headers to include in webhook requests,
	// typically used for authentication tokens (e.g., "Authorization": "Bearer <token>").
	Headers map[string]string

	// BatchSize is the number of events to accumulate before flushing to the webhook.
	// Defaults to DefaultBatchSize if zero.
	BatchSize int

	// FlushInterval is the maximum time to wait before flushing a partial batch.
	// Defaults to DefaultFlushInterval if zero.
	FlushInterval time.Duration

	// Timeout is the HTTP request timeout for each webhook call.
	// Defaults to DefaultTimeout if zero.
	Timeout time.Duration

	// CloudEvents controls whether events are wrapped in CloudEvents v1.0 format.
	CloudEvents bool
}

// WithDefaults returns a copy of the config with zero-value fields set to their defaults.
func (c Config) WithDefaults() Config {
	if c.BatchSize <= 0 {
		c.BatchSize = DefaultBatchSize
	}
	if c.FlushInterval <= 0 {
		c.FlushInterval = DefaultFlushInterval
	}
	if c.Timeout <= 0 {
		c.Timeout = DefaultTimeout
	}
	return c
}
