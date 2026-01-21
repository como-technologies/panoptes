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

package daemon

import (
	"context"
	"fmt"
	"sync"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/health/grpc_health_v1"
	"sigs.k8s.io/controller-runtime/pkg/log"
)

const (
	// DefaultDaemonPort is the default gRPC port for argusd
	DefaultDaemonPort = 50051

	// DefaultRequestTimeout is the default timeout for gRPC requests
	DefaultRequestTimeout = 10 * time.Second
)

// ErrCircuitOpen is returned when the circuit breaker is open for a node.
// This means the node has had multiple consecutive failures and requests
// are being rejected to prevent cascading delays.
//
// The caller should:
//   - Skip this node in reconciliation
//   - Continue processing other nodes
//   - Requeue with backoff to retry later
type ErrCircuitOpen struct {
	NodeIP   string
	Failures int
}

func (e *ErrCircuitOpen) Error() string {
	return fmt.Sprintf("circuit open for node %s (failures: %d)", e.NodeIP, e.Failures)
}

// Client manages connections to argusd daemons running on nodes.
//
// Includes per-node circuit breakers to prevent cascading failures when
// a daemon becomes unavailable. See circuit.go for circuit breaker details.
type Client struct {
	mu          sync.RWMutex
	connections map[string]*grpc.ClientConn
	circuits    map[string]*CircuitBreaker
	port        int
}

// NewClient creates a new daemon client manager.
//
// Circuit breakers are automatically created per-node with default settings
// (5 failures to open, 30s reset timeout).
func NewClient(port int) *Client {
	if port == 0 {
		port = DefaultDaemonPort
	}
	return &Client{
		connections: make(map[string]*grpc.ClientConn),
		circuits:    make(map[string]*CircuitBreaker),
		port:        port,
	}
}

// GetConnection returns a gRPC connection to the daemon on the specified node.
//
// Uses a per-node circuit breaker to prevent cascading failures. If the circuit
// is open (node has had multiple recent failures), returns ErrCircuitOpen
// immediately without attempting connection.
//
// # Circuit Breaker Behavior
//
// - Closed (normal): Requests flow through, failures counted
// - Open (tripped): Immediate ErrCircuitOpen after threshold failures
// - Half-open (probe): One request allowed through after reset timeout
//
// Use RecordSuccess/RecordFailure to update circuit state after operations.
func (c *Client) GetConnection(ctx context.Context, nodeIP string) (*grpc.ClientConn, error) {
	logger := log.FromContext(ctx)

	// Check circuit breaker BEFORE attempting connection
	cb := c.getOrCreateCircuit(nodeIP)
	if !cb.Allow() {
		logger.V(1).Info("Circuit open, skipping node",
			"nodeIP", nodeIP,
			"failures", cb.Failures(),
			"state", cb.State().String())
		return nil, &ErrCircuitOpen{
			NodeIP:   nodeIP,
			Failures: cb.Failures(),
		}
	}

	c.mu.RLock()
	conn, exists := c.connections[nodeIP]
	c.mu.RUnlock()

	if exists && conn.GetState().String() != "SHUTDOWN" {
		return conn, nil
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	// Double-check after acquiring write lock
	if conn, exists := c.connections[nodeIP]; exists && conn.GetState().String() != "SHUTDOWN" {
		return conn, nil
	}

	target := fmt.Sprintf("%s:%d", nodeIP, c.port)

	logger.V(1).Info("Connecting to argusd", "target", target)

	conn, err := grpc.NewClient(target,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		cb.RecordFailure()
		logger.V(1).Info("Connection failed, circuit updated",
			"nodeIP", nodeIP,
			"error", err.Error(),
			"failures", cb.Failures(),
			"state", cb.State().String())
		return nil, fmt.Errorf("failed to create client for argusd at %s: %w", target, err)
	}

	c.connections[nodeIP] = conn
	return conn, nil
}

// getOrCreateCircuit returns the circuit breaker for a node, creating one if needed.
func (c *Client) getOrCreateCircuit(nodeIP string) *CircuitBreaker {
	c.mu.RLock()
	cb, exists := c.circuits[nodeIP]
	c.mu.RUnlock()

	if exists {
		return cb
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	// Double-check after acquiring write lock
	if cb, exists := c.circuits[nodeIP]; exists {
		return cb
	}

	cb = NewCircuitBreaker()
	c.circuits[nodeIP] = cb
	return cb
}

// CheckHealth verifies the daemon is healthy on the specified node.
//
// Updates the circuit breaker based on the result:
//   - Success: Records success, may close circuit if half-open
//   - Failure: Records failure, may open circuit if threshold reached
func (c *Client) CheckHealth(ctx context.Context, nodeIP string) error {
	conn, err := c.GetConnection(ctx, nodeIP)
	if err != nil {
		return err
	}

	cb := c.getOrCreateCircuit(nodeIP)
	healthClient := grpc_health_v1.NewHealthClient(conn)

	reqCtx, cancel := context.WithTimeout(ctx, DefaultRequestTimeout)
	defer cancel()

	resp, err := healthClient.Check(reqCtx, &grpc_health_v1.HealthCheckRequest{
		Service: "argusd",
	})
	if err != nil {
		cb.RecordFailure()
		return fmt.Errorf("health check failed: %w", err)
	}

	if resp.Status != grpc_health_v1.HealthCheckResponse_SERVING {
		cb.RecordFailure()
		return fmt.Errorf("daemon not serving: status=%s", resp.Status.String())
	}

	cb.RecordSuccess()
	return nil
}

// RecordSuccess records a successful operation for the node's circuit breaker.
//
// Call this after a successful gRPC call to the daemon.
func (c *Client) RecordSuccess(nodeIP string) {
	cb := c.getOrCreateCircuit(nodeIP)
	cb.RecordSuccess()
}

// RecordFailure records a failed operation for the node's circuit breaker.
//
// Call this after a failed gRPC call to the daemon.
func (c *Client) RecordFailure(nodeIP string) {
	cb := c.getOrCreateCircuit(nodeIP)
	cb.RecordFailure()
}

// ResetCircuit manually resets the circuit breaker for a node.
//
// Use this when you know the daemon has recovered (e.g., pod restarted).
func (c *Client) ResetCircuit(nodeIP string) {
	cb := c.getOrCreateCircuit(nodeIP)
	cb.Reset()
}

// GetCircuitState returns the current circuit breaker state for a node.
//
// Returns CircuitClosed if no circuit exists for the node.
func (c *Client) GetCircuitState(nodeIP string) CircuitState {
	c.mu.RLock()
	cb, exists := c.circuits[nodeIP]
	c.mu.RUnlock()

	if !exists {
		return CircuitClosed
	}
	return cb.State()
}

// IsCircuitOpen returns true if the circuit is open for the node.
//
// Useful for checking if a node should be skipped in reconciliation.
func (c *Client) IsCircuitOpen(nodeIP string) bool {
	return c.GetCircuitState(nodeIP) == CircuitOpen
}

// CloseConnection closes the connection to a specific node.
func (c *Client) CloseConnection(nodeIP string) error {
	c.mu.Lock()
	defer c.mu.Unlock()

	if conn, exists := c.connections[nodeIP]; exists {
		delete(c.connections, nodeIP)
		return conn.Close()
	}
	return nil
}

// Close closes all daemon connections.
func (c *Client) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	var lastErr error
	for nodeIP, conn := range c.connections {
		if err := conn.Close(); err != nil {
			lastErr = err
		}
		delete(c.connections, nodeIP)
	}
	return lastErr
}

// NodeCount returns the number of connected nodes.
func (c *Client) NodeCount() int {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.connections)
}
