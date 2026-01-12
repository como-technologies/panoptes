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

	// DefaultConnectTimeout is the default timeout for connecting to daemons
	DefaultConnectTimeout = 5 * time.Second

	// DefaultRequestTimeout is the default timeout for gRPC requests
	DefaultRequestTimeout = 10 * time.Second
)

// Client manages connections to argusd daemons running on nodes.
type Client struct {
	mu          sync.RWMutex
	connections map[string]*grpc.ClientConn
	port        int
}

// NewClient creates a new daemon client manager.
func NewClient(port int) *Client {
	if port == 0 {
		port = DefaultDaemonPort
	}
	return &Client{
		connections: make(map[string]*grpc.ClientConn),
		port:        port,
	}
}

// GetConnection returns a gRPC connection to the daemon on the specified node.
// Connections are cached and reused.
func (c *Client) GetConnection(ctx context.Context, nodeIP string) (*grpc.ClientConn, error) {
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

	logger := log.FromContext(ctx)
	target := fmt.Sprintf("%s:%d", nodeIP, c.port)

	logger.V(1).Info("Connecting to argusd", "target", target)

	dialCtx, cancel := context.WithTimeout(ctx, DefaultConnectTimeout)
	defer cancel()

	conn, err := grpc.DialContext(dialCtx, target,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
	)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to argusd at %s: %w", target, err)
	}

	c.connections[nodeIP] = conn
	return conn, nil
}

// CheckHealth verifies the daemon is healthy on the specified node.
func (c *Client) CheckHealth(ctx context.Context, nodeIP string) error {
	conn, err := c.GetConnection(ctx, nodeIP)
	if err != nil {
		return err
	}

	healthClient := grpc_health_v1.NewHealthClient(conn)

	reqCtx, cancel := context.WithTimeout(ctx, DefaultRequestTimeout)
	defer cancel()

	resp, err := healthClient.Check(reqCtx, &grpc_health_v1.HealthCheckRequest{
		Service: "argusd",
	})
	if err != nil {
		return fmt.Errorf("health check failed: %w", err)
	}

	if resp.Status != grpc_health_v1.HealthCheckResponse_SERVING {
		return fmt.Errorf("daemon not serving: status=%s", resp.Status.String())
	}

	return nil
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
