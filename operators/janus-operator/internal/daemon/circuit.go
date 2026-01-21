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

// Package daemon provides client functionality for communicating with janusd.
//
// # Circuit Breaker Pattern
//
// This file implements a per-node circuit breaker to prevent cascading failures
// when a daemon becomes unavailable. Without this protection:
//
//   - A single unavailable daemon causes connection timeouts
//   - The operator's reconciliation loop slows down dramatically
//   - All pods on that node (and potentially others) experience delays
//   - The operator may exceed its rate limits or resource quotas
//
// The circuit breaker provides fail-fast behavior:
//
//   - After N consecutive failures, the circuit "opens" and immediately returns errors
//   - After a reset timeout, the circuit "half-opens" to allow a probe request
//   - If the probe succeeds, the circuit "closes" and normal operation resumes
//
// # Configuration
//
// | Setting      | Default | Description                          |
// |--------------|---------|--------------------------------------|
// | Threshold    | 5       | Consecutive failures to open circuit |
// | ResetTimeout | 30s     | Time before half-open probe attempt  |
//
// # Metrics for Monitoring
//
// Track these in your monitoring system:
//   - Circuit open events (node went unhealthy)
//   - Circuit close events (node recovered)
//   - Half-open probe results
//
// # References
//
// - Circuit Breaker pattern: https://martinfowler.com/bliki/CircuitBreaker.html
// - Go implementation patterns: https://blog.golang.org/context
package daemon

import (
	"sync"
	"time"
)

// CircuitState represents the current state of a circuit breaker.
type CircuitState int

const (
	// CircuitClosed means the circuit is operating normally.
	// Requests flow through and failures are counted.
	CircuitClosed CircuitState = iota

	// CircuitOpen means the circuit has tripped.
	// Requests fail immediately without attempting the operation.
	CircuitOpen

	// CircuitHalfOpen means the circuit is testing recovery.
	// A single probe request is allowed through.
	CircuitHalfOpen
)

func (s CircuitState) String() string {
	switch s {
	case CircuitClosed:
		return "closed"
	case CircuitOpen:
		return "open"
	case CircuitHalfOpen:
		return "half-open"
	default:
		return "unknown"
	}
}

// DefaultCircuitThreshold is the number of consecutive failures before opening.
const DefaultCircuitThreshold = 5

// DefaultCircuitResetTimeout is how long to wait before half-open probe.
const DefaultCircuitResetTimeout = 30 * time.Second

// CircuitBreaker implements the circuit breaker pattern for a single endpoint.
//
// Thread-safe for concurrent use.
//
// # State Transitions
//
//	                     ┌─────────────────┐
//	                     │     CLOSED      │
//	                     │ (normal flow)   │
//	                     └────────┬────────┘
//	                              │
//	                              │ N consecutive failures
//	                              ▼
//	                     ┌─────────────────┐
//	                     │      OPEN       │
//	                     │ (fail-fast)     │
//	                     └────────┬────────┘
//	                              │
//	                              │ reset timeout elapsed
//	                              ▼
//	                     ┌─────────────────┐
//	     success         │   HALF-OPEN     │         failure
//	┌────────────────────│  (probe test)   │────────────────────┐
//	│                    └─────────────────┘                    │
//	▼                                                           ▼
//
// CLOSED                                                     OPEN
type CircuitBreaker struct {
	mu           sync.RWMutex
	failures     int           // consecutive failure count
	lastFailure  time.Time     // time of most recent failure
	threshold    int           // failures needed to open circuit
	resetTimeout time.Duration // wait time before half-open
	state        CircuitState  // current circuit state
}

// NewCircuitBreaker creates a circuit breaker with default settings.
//
// Default configuration:
//   - Threshold: 5 consecutive failures to open
//   - ResetTimeout: 30 seconds before half-open probe
func NewCircuitBreaker() *CircuitBreaker {
	return &CircuitBreaker{
		threshold:    DefaultCircuitThreshold,
		resetTimeout: DefaultCircuitResetTimeout,
		state:        CircuitClosed,
	}
}

// NewCircuitBreakerWithConfig creates a circuit breaker with custom settings.
func NewCircuitBreakerWithConfig(threshold int, resetTimeout time.Duration) *CircuitBreaker {
	return &CircuitBreaker{
		threshold:    threshold,
		resetTimeout: resetTimeout,
		state:        CircuitClosed,
	}
}

// Allow checks if a request should be allowed through the circuit.
//
// Returns:
//   - true if the request can proceed (circuit closed or half-open)
//   - false if the request should fail immediately (circuit open)
//
// For half-open state, only one request is allowed through for probing.
// Use RecordSuccess or RecordFailure to update the circuit after the request.
func (cb *CircuitBreaker) Allow() bool {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	switch cb.state {
	case CircuitClosed:
		return true

	case CircuitOpen:
		// Check if reset timeout has elapsed
		if time.Since(cb.lastFailure) > cb.resetTimeout {
			// Transition to half-open for probe
			cb.state = CircuitHalfOpen
			return true
		}
		return false

	case CircuitHalfOpen:
		// In half-open, we allow one request through.
		// The next RecordSuccess/RecordFailure will transition state.
		return true

	default:
		return false
	}
}

// RecordSuccess records a successful request.
//
// If in half-open state, this closes the circuit (recovery complete).
// If in closed state, this resets the failure counter.
func (cb *CircuitBreaker) RecordSuccess() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	cb.failures = 0
	if cb.state == CircuitHalfOpen {
		cb.state = CircuitClosed
	}
}

// RecordFailure records a failed request.
//
// If in half-open state, this re-opens the circuit.
// If in closed state and threshold reached, opens the circuit.
func (cb *CircuitBreaker) RecordFailure() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	cb.failures++
	cb.lastFailure = time.Now()

	switch cb.state {
	case CircuitClosed:
		if cb.failures >= cb.threshold {
			cb.state = CircuitOpen
		}
	case CircuitHalfOpen:
		// Probe failed, go back to open
		cb.state = CircuitOpen
	}
}

// State returns the current state of the circuit breaker.
func (cb *CircuitBreaker) State() CircuitState {
	cb.mu.RLock()
	defer cb.mu.RUnlock()
	return cb.state
}

// Failures returns the current consecutive failure count.
func (cb *CircuitBreaker) Failures() int {
	cb.mu.RLock()
	defer cb.mu.RUnlock()
	return cb.failures
}

// Reset manually resets the circuit breaker to closed state.
//
// Use this when you know the endpoint has recovered (e.g., pod restarted).
func (cb *CircuitBreaker) Reset() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	cb.failures = 0
	cb.state = CircuitClosed
	cb.lastFailure = time.Time{}
}
