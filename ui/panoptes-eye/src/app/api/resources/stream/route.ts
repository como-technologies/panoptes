import { NextRequest } from 'next/server';
import * as k8s from '@kubernetes/client-node';
import type { ArgusWatcher } from '@/types/argus';
import type { JanusGuard } from '@/types/janus';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

// CRD configuration
const ARGUS_GROUP = 'argus.como-technologies.io';
const ARGUS_VERSION = 'v1';
const ARGUS_PLURAL = 'arguswatchers';

const JANUS_GROUP = 'janus.como-technologies.io';
const JANUS_VERSION = 'v1';
const JANUS_PLURAL = 'janusguards';

// Initialize Kubernetes configuration
function getK8sConfig(): k8s.KubeConfig | null {
  const kc = new k8s.KubeConfig();
  try {
    kc.loadFromDefault();
    return kc;
  } catch {
    try {
      kc.loadFromCluster();
      return kc;
    } catch {
      console.warn('K8s Watch: No configuration found');
      return null;
    }
  }
}

interface ResourceEvent {
  type: 'watcher' | 'guard';
  action: 'ADDED' | 'MODIFIED' | 'DELETED';
  resource: ArgusWatcher | JanusGuard;
}

export async function GET(request: NextRequest) {
  const encoder = new TextEncoder();
  const kc = getK8sConfig();

  if (!kc) {
    return new Response(
      JSON.stringify({ error: 'Kubernetes not configured' }),
      { status: 503, headers: { 'Content-Type': 'application/json' } }
    );
  }

  const watch = new k8s.Watch(kc);
  const abortControllers: AbortController[] = [];

  const stream = new ReadableStream({
    async start(controller) {
      // Send initial connection message
      controller.enqueue(encoder.encode(`data: ${JSON.stringify({
        type: 'connected',
        message: 'Resource stream connected',
        watching: ['arguswatchers', 'janusguards'],
      })}\n\n`));

      const sendEvent = (event: ResourceEvent) => {
        try {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({
            type: 'resource',
            data: event,
          })}\n\n`));
        } catch {
          // Stream was closed
        }
      };

      const sendError = (source: string, message: string) => {
        try {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({
            type: 'error',
            source,
            message,
          })}\n\n`));
        } catch {
          // Stream was closed
        }
      };

      // Watch ArgusWatchers across all namespaces
      try {
        const watcherAbort = new AbortController();
        abortControllers.push(watcherAbort);

        const watcherPath = `/apis/${ARGUS_GROUP}/${ARGUS_VERSION}/${ARGUS_PLURAL}`;
        console.log(`Resource stream: Watching ${watcherPath}`);

        await watch.watch(
          watcherPath,
          {},
          (type, apiObj) => {
            const action = type as 'ADDED' | 'MODIFIED' | 'DELETED';
            sendEvent({
              type: 'watcher',
              action,
              resource: apiObj as ArgusWatcher,
            });
          },
          (err) => {
            if (err) {
              console.error('ArgusWatcher watch error:', err);
              sendError('watcher', err.message || 'Watch error');
            }
          }
        );
      } catch (err) {
        console.error('Failed to start ArgusWatcher watch:', err);
        sendError('watcher', err instanceof Error ? err.message : 'Failed to start watch');
      }

      // Watch JanusGuards across all namespaces
      try {
        const guardAbort = new AbortController();
        abortControllers.push(guardAbort);

        const guardPath = `/apis/${JANUS_GROUP}/${JANUS_VERSION}/${JANUS_PLURAL}`;
        console.log(`Resource stream: Watching ${guardPath}`);

        await watch.watch(
          guardPath,
          {},
          (type, apiObj) => {
            const action = type as 'ADDED' | 'MODIFIED' | 'DELETED';
            sendEvent({
              type: 'guard',
              action,
              resource: apiObj as JanusGuard,
            });
          },
          (err) => {
            if (err) {
              console.error('JanusGuard watch error:', err);
              sendError('guard', err.message || 'Watch error');
            }
          }
        );
      } catch (err) {
        console.error('Failed to start JanusGuard watch:', err);
        sendError('guard', err instanceof Error ? err.message : 'Failed to start watch');
      }

      // Send periodic heartbeat to keep connection alive
      const heartbeat = setInterval(() => {
        try {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({
            type: 'heartbeat',
            timestamp: new Date().toISOString(),
          })}\n\n`));
        } catch {
          clearInterval(heartbeat);
        }
      }, 30000);

      // Handle client disconnect
      request.signal.addEventListener('abort', () => {
        console.log('Resource stream: Client disconnected');
        clearInterval(heartbeat);

        // Abort all watch connections
        for (const ac of abortControllers) {
          try {
            ac.abort();
          } catch {
            // Already aborted
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
