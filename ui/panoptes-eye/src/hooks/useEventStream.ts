'use client';

import { useEffect, useRef, useCallback } from 'react';
import { useEventStats } from '@/stores/eventStats';

interface StreamEvent {
  id: string;
  timestamp: string;
  source: 'argus' | 'janus';
  eventType: string;
  path: string;
  podName: string;
  action: 'allowed' | 'denied' | 'audit' | 'detected';
  nodeName: string;
  namespace?: string;
}

interface UseEventStreamOptions {
  enabled?: boolean;
  onEvent?: (event: StreamEvent) => void;
  onConnect?: () => void;
  onDisconnect?: () => void;
}

export function useEventStream(options: UseEventStreamOptions = {}) {
  const { enabled = true, onEvent, onConnect, onDisconnect } = options;
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Access store actions via getState() to avoid calling hooks during render
  // This prevents SSR/hydration issues
  const handleEvent = useCallback(
    (event: StreamEvent) => {
      const { incrementArgus, incrementJanus, addAlert } = useEventStats.getState();

      // Update Zustand store with event counts
      if (event.source === 'argus') {
        incrementArgus();
      } else if (event.source === 'janus') {
        const action = event.action === 'detected' ? 'audit' : event.action;
        incrementJanus(action as 'allowed' | 'denied' | 'audit');

        // Add alert for denied events
        if (event.action === 'denied') {
          addAlert({
            type: 'critical',
            message: `Access denied: ${event.path}`,
            path: event.path,
            podName: event.podName,
          });
        }
      }

      // Call user callback if provided
      onEvent?.(event);
    },
    [onEvent]
  );

  const connect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }

    const es = new EventSource('/api/events/stream');
    eventSourceRef.current = es;

    es.onopen = () => {
      onConnect?.();
    };

    es.onmessage = (messageEvent) => {
      try {
        const data = JSON.parse(messageEvent.data);
        if (data.type === 'connected') {
          onConnect?.();
        } else if (data.type === 'event' && data.data) {
          handleEvent(data.data);
        }
      } catch (err) {
        console.error('Failed to parse event:', err);
      }
    };

    es.onerror = () => {
      onDisconnect?.();
      es.close();
      // Attempt to reconnect after a delay
      reconnectTimeoutRef.current = setTimeout(connect, 5000);
    };
  }, [handleEvent, onConnect, onDisconnect]);

  useEffect(() => {
    if (enabled) {
      connect();
    }

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
    };
  }, [enabled, connect]);

  const disconnect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
  }, []);

  return { disconnect };
}
