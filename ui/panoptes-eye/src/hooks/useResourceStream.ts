'use client';

import { useEffect, useRef, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';

interface ResourceEvent {
  type: 'watcher' | 'guard';
  action: 'ADDED' | 'MODIFIED' | 'DELETED';
  resource: unknown;
}

interface StreamMessage {
  type: 'connected' | 'resource' | 'heartbeat' | 'error';
  data?: ResourceEvent;
  message?: string;
  source?: string;
}

interface UseResourceStreamOptions {
  /** Whether to enable the stream (default: true) */
  enabled?: boolean;
  /** Callback when a resource event is received */
  onEvent?: (event: ResourceEvent) => void;
  /** Callback when connection status changes */
  onConnectionChange?: (connected: boolean) => void;
}

/**
 * Hook that subscribes to the K8s resource stream via SSE
 * and automatically invalidates React Query caches when resources change.
 */
export function useResourceStream(options: UseResourceStreamOptions = {}) {
  const { enabled = true, onEvent, onConnectionChange } = options;
  const queryClient = useQueryClient();
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const connectedRef = useRef(false);

  const invalidateQueries = useCallback((event: ResourceEvent) => {
    // Invalidate relevant queries based on resource type
    if (event.type === 'watcher') {
      queryClient.invalidateQueries({ queryKey: ['watchers'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    } else if (event.type === 'guard') {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    }
  }, [queryClient]);

  const connect = useCallback(() => {
    if (eventSourceRef.current) {
      return;
    }

    console.log('Resource stream: Connecting...');
    const eventSource = new EventSource('/api/resources/stream');
    eventSourceRef.current = eventSource;

    eventSource.onopen = () => {
      console.log('Resource stream: Connected');
      connectedRef.current = true;
      onConnectionChange?.(true);
    };

    eventSource.onmessage = (event) => {
      try {
        const message: StreamMessage = JSON.parse(event.data);

        switch (message.type) {
          case 'connected':
            console.log('Resource stream: Watching resources');
            break;

          case 'resource':
            if (message.data) {
              console.log(`Resource stream: ${message.data.type} ${message.data.action}`);
              invalidateQueries(message.data);
              onEvent?.(message.data);
            }
            break;

          case 'heartbeat':
            // Keep-alive, no action needed
            break;

          case 'error':
            console.error(`Resource stream error (${message.source}):`, message.message);
            break;
        }
      } catch (err) {
        console.error('Resource stream: Failed to parse message', err);
      }
    };

    eventSource.onerror = () => {
      console.log('Resource stream: Disconnected, will reconnect...');
      connectedRef.current = false;
      onConnectionChange?.(false);

      // Close the current connection
      eventSource.close();
      eventSourceRef.current = null;

      // Reconnect after delay
      if (enabled) {
        reconnectTimeoutRef.current = setTimeout(() => {
          connect();
        }, 5000);
      }
    };
  }, [enabled, invalidateQueries, onEvent, onConnectionChange]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }

    if (eventSourceRef.current) {
      console.log('Resource stream: Disconnecting');
      eventSourceRef.current.close();
      eventSourceRef.current = null;
      connectedRef.current = false;
      onConnectionChange?.(false);
    }
  }, [onConnectionChange]);

  useEffect(() => {
    if (enabled) {
      connect();
    } else {
      disconnect();
    }

    return () => {
      disconnect();
    };
  }, [enabled, connect, disconnect]);

  return {
    connected: connectedRef.current,
    disconnect,
    reconnect: connect,
  };
}
