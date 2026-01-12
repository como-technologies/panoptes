import { NextRequest } from 'next/server';
import * as grpc from '@grpc/grpc-js';
import {
  streamArgusEventsFromEndpoint,
  streamJanusEventsFromEndpoint,
  fileEventToUnified,
  accessEventToUnified,
  type UnifiedEvent,
  type FileEvent,
  type AccessEvent,
} from '@/lib/grpc';
import { getDaemonPodEndpoints, type DaemonEndpoints } from '@/lib/k8s';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

// Detect if running outside Kubernetes cluster (local development)
const isLocalDev = !process.env.KUBERNETES_SERVICE_HOST;

// Generate mock events when daemons are unavailable
function generateMockEvent(eventId: number): UnifiedEvent {
  const eventTypes = ['access', 'modify', 'create', 'delete', 'open', 'close'];
  const paths = [
    '/etc/passwd',
    '/etc/shadow',
    '/var/log/auth.log',
    '/var/log/syslog',
    '/tmp/data.txt',
    '/app/config.json',
    '/home/user/.bashrc',
  ];
  const pods = [
    'api-server-abc123',
    'backend-xyz789',
    'worker-def456',
    'nginx-ingress-5bf',
    'redis-master-0',
  ];
  const actions = ['allowed', 'denied', 'audit', 'detected'] as const;

  return {
    id: `mock-${eventId}`,
    timestamp: new Date().toISOString(),
    source: Math.random() > 0.5 ? 'argus' : 'janus',
    eventType: eventTypes[Math.floor(Math.random() * eventTypes.length)],
    path: paths[Math.floor(Math.random() * paths.length)],
    podName: pods[Math.floor(Math.random() * pods.length)],
    nodeName: 'node-1',
    namespace: 'default',
    action: actions[Math.floor(Math.random() * actions.length)],
  };
}

export async function GET(request: NextRequest) {
  const encoder = new TextEncoder();
  let eventId = 0;

  // Discover daemon endpoints - use localhost for local dev, pod IPs for in-cluster
  let argusdEndpoints: DaemonEndpoints;
  let janusdEndpoints: DaemonEndpoints;

  if (isLocalDev) {
    // Local development: use localhost (requires port-forward)
    argusdEndpoints = {
      pods: [],
      endpoints: [process.env.ARGUSD_HOST || 'localhost:50051'],
    };
    janusdEndpoints = {
      pods: [],
      endpoints: [process.env.JANUSD_HOST || 'localhost:50052'],
    };
    console.log('Event stream: Local dev mode - using localhost endpoints (requires port-forward)');
  } else {
    // In-cluster: discover pod IPs dynamically
    [argusdEndpoints, janusdEndpoints] = await Promise.all([
      getDaemonPodEndpoints('argusd'),
      getDaemonPodEndpoints('janusd'),
    ]);
    console.log(`Event stream: Found ${argusdEndpoints.endpoints.length} argusd pods, ${janusdEndpoints.endpoints.length} janusd pods`);
  }

  const useMockData = argusdEndpoints.endpoints.length === 0 && janusdEndpoints.endpoints.length === 0;

  const stream = new ReadableStream({
    async start(controller) {
      // Send initial connection message with status
      controller.enqueue(encoder.encode(`data: ${JSON.stringify({
        type: 'connected',
        daemons: {
          argusd: argusdEndpoints.endpoints.length > 0,
          janusd: janusdEndpoints.endpoints.length > 0,
          argusdPods: argusdEndpoints.pods.map(p => ({ name: p.name, node: p.nodeName })),
          janusdPods: janusdEndpoints.pods.map(p => ({ name: p.name, node: p.nodeName })),
        },
        mockMode: useMockData,
      })}\n\n`));

      const sendEvent = (event: UnifiedEvent) => {
        try {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({ type: 'event', data: event })}\n\n`));
        } catch {
          // Stream was closed
        }
      };

      // If no daemons available, fall back to mock data
      if (useMockData) {
        console.warn('No daemons available, using mock event data');

        const interval = setInterval(() => {
          sendEvent(generateMockEvent(++eventId));
        }, 2000 + Math.random() * 3000);

        request.signal.addEventListener('abort', () => {
          clearInterval(interval);
          try {
            controller.close();
          } catch {
            // Already closed
          }
        });

        return;
      }

      // Track all active streams for cleanup
      const activeStreams: grpc.ClientReadableStream<FileEvent | AccessEvent>[] = [];

      // Connect to ALL Argus daemon pods
      for (const endpoint of argusdEndpoints.endpoints) {
        console.log(`Connecting to Argusd at ${endpoint}...`);
        const argusStream = await streamArgusEventsFromEndpoint(
          endpoint,
          (event) => {
            sendEvent(fileEventToUnified(event, `argus-${++eventId}`));
          },
          (error) => {
            console.error(`Argus stream error from ${endpoint}:`, error);
            try {
              controller.enqueue(encoder.encode(`data: ${JSON.stringify({
                type: 'error',
                source: 'argus',
                endpoint,
                message: error.message,
              })}\n\n`));
            } catch {
              // Stream closed
            }
          }
        );
        if (argusStream) {
          activeStreams.push(argusStream);
        }
      }

      // Connect to ALL Janus daemon pods
      for (const endpoint of janusdEndpoints.endpoints) {
        console.log(`Connecting to Janusd at ${endpoint}...`);
        const janusStream = await streamJanusEventsFromEndpoint(
          endpoint,
          (event) => {
            sendEvent(accessEventToUnified(event, `janus-${++eventId}`));
          },
          (error) => {
            console.error(`Janus stream error from ${endpoint}:`, error);
            try {
              controller.enqueue(encoder.encode(`data: ${JSON.stringify({
                type: 'error',
                source: 'janus',
                endpoint,
                message: error.message,
              })}\n\n`));
            } catch {
              // Stream closed
            }
          },
          { includeAllowed: true }
        );
        if (janusStream) {
          activeStreams.push(janusStream);
        }
      }

      console.log(`Event stream: Connected to ${activeStreams.length} daemon streams`);

      // Send periodic heartbeat to keep connection alive
      const heartbeat = setInterval(() => {
        try {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({
            type: 'heartbeat',
            activeStreams: activeStreams.length,
          })}\n\n`));
        } catch {
          clearInterval(heartbeat);
        }
      }, 30000);

      // Handle client disconnect
      request.signal.addEventListener('abort', () => {
        clearInterval(heartbeat);

        // Cancel all gRPC streams
        for (const stream of activeStreams) {
          try {
            stream.cancel();
          } catch {
            // Already cancelled
          }
        }

        try {
          controller.close();
        } catch {
          // Already closed
        }
      });
    },
  });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive',
    },
  });
}
