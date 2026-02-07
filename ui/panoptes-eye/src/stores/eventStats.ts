'use client';

import { create } from 'zustand';
import { useState, useEffect } from 'react';

export interface Alert {
  id: string;
  type: 'critical' | 'warning';
  message: string;
  path?: string;
  podName?: string;
  timestamp: string;
}

export interface ProcessInfo {
  pid?: number;
  tid?: number;
  uid?: number;
  gid?: number;
  comm?: string;
  exe?: string;
  // V2 fields (Rust daemon only)
  ppid?: number;
  cmdline?: string[];
  cwd?: string;
}

export interface StreamEvent {
  id: string;
  timestamp: string;
  source: 'argus' | 'janus';
  /** The name of the ArgusWatcher or JanusGuard that generated this event */
  resourceName: string;
  eventType: string;
  path: string;
  podName: string;
  action: 'allowed' | 'denied' | 'audit' | 'detected';
  nodeName: string;
  namespace?: string;
  // V2 field - populated by Rust daemons (Janus always, Argus with eBPF)
  processInfo?: ProcessInfo;
}

interface EventStatsState {
  // Counts
  argusEvents: number;
  janusEvents: number;
  deniedEvents: number;
  allowedEvents: number;
  auditEvents: number;

  // Events
  recentEvents: StreamEvent[];

  // Alerts
  alerts: Alert[];
}

interface EventStatsActions {
  incrementArgus: () => void;
  incrementJanus: (action: 'allowed' | 'denied' | 'audit') => void;
  addEvent: (event: StreamEvent) => void;
  clearEvents: () => void;
  addAlert: (alert: Omit<Alert, 'id' | 'timestamp'>) => void;
  dismissAlert: (id: string) => void;
  clearAlerts: () => void;
  resetStats: () => void;
}

type EventStatsStore = EventStatsState & EventStatsActions;

const MAX_ALERTS = 100;
const MAX_EVENTS = 500;

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Simple Zustand store without persist middleware to avoid SSR issues
export const useEventStats = create<EventStatsStore>()((set) => ({
  // Initial state - all zeros
  argusEvents: 0,
  janusEvents: 0,
  deniedEvents: 0,
  allowedEvents: 0,
  auditEvents: 0,
  recentEvents: [],
  alerts: [],

  incrementArgus: () => {
    set((state) => ({ argusEvents: state.argusEvents + 1 }));
  },

  incrementJanus: (action: 'allowed' | 'denied' | 'audit') => {
    set((state) => {
      const updates: Partial<EventStatsState> = {
        janusEvents: state.janusEvents + 1,
      };
      if (action === 'denied') {
        updates.deniedEvents = state.deniedEvents + 1;
      } else if (action === 'allowed') {
        updates.allowedEvents = state.allowedEvents + 1;
      } else if (action === 'audit') {
        updates.auditEvents = state.auditEvents + 1;
      }
      return updates;
    });
  },

  addEvent: (event) => {
    set((state) => {
      const events = [event, ...state.recentEvents];
      if (events.length > MAX_EVENTS) {
        events.splice(MAX_EVENTS);
      }
      return { recentEvents: events };
    });
  },

  clearEvents: () => {
    set({ recentEvents: [] });
  },

  addAlert: (alert) => {
    const newAlert: Alert = {
      ...alert,
      id: generateId(),
      timestamp: new Date().toISOString(),
    };
    set((state) => {
      const alerts = [newAlert, ...state.alerts];
      // Keep only the most recent MAX_ALERTS
      if (alerts.length > MAX_ALERTS) {
        alerts.splice(MAX_ALERTS);
      }
      return { alerts };
    });
  },

  dismissAlert: (id) => {
    set((state) => ({
      alerts: state.alerts.filter((a) => a.id !== id),
    }));
  },

  clearAlerts: () => {
    set({ alerts: [] });
  },

  resetStats: () => {
    set({
      argusEvents: 0,
      janusEvents: 0,
      deniedEvents: 0,
      allowedEvents: 0,
      auditEvents: 0,
      recentEvents: [],
      alerts: [],
    });
  },
}));

// SSR-safe selector hooks - NEVER call Zustand during render
// Only subscribe to store changes after client mount
interface EventCounts {
  argusEvents: number;
  janusEvents: number;
  deniedEvents: number;
  allowedEvents: number;
  auditEvents: number;
  total: number;
}

const defaultCounts: EventCounts = {
  argusEvents: 0,
  janusEvents: 0,
  deniedEvents: 0,
  allowedEvents: 0,
  auditEvents: 0,
  total: 0,
};

export function useEventCounts(): EventCounts {
  const [counts, setCounts] = useState<EventCounts>(defaultCounts);

  useEffect(() => {
    // Get initial state on mount
    const state = useEventStats.getState();
    setCounts({
      argusEvents: state.argusEvents,
      janusEvents: state.janusEvents,
      deniedEvents: state.deniedEvents,
      allowedEvents: state.allowedEvents,
      auditEvents: state.auditEvents,
      total: state.argusEvents + state.janusEvents,
    });

    // Subscribe to changes
    const unsubscribe = useEventStats.subscribe((state) => {
      setCounts({
        argusEvents: state.argusEvents,
        janusEvents: state.janusEvents,
        deniedEvents: state.deniedEvents,
        allowedEvents: state.allowedEvents,
        auditEvents: state.auditEvents,
        total: state.argusEvents + state.janusEvents,
      });
    });

    return unsubscribe;
  }, []);

  return counts;
}

interface AlertsData {
  alerts: Alert[];
  criticalCount: number;
  warningCount: number;
  totalCount: number;
}

const defaultAlerts: AlertsData = {
  alerts: [],
  criticalCount: 0,
  warningCount: 0,
  totalCount: 0,
};

export function useAlerts(): AlertsData {
  const [alertsData, setAlertsData] = useState<AlertsData>(defaultAlerts);

  useEffect(() => {
    // Get initial state on mount
    const state = useEventStats.getState();
    setAlertsData({
      alerts: state.alerts,
      criticalCount: state.alerts.filter((a) => a.type === 'critical').length,
      warningCount: state.alerts.filter((a) => a.type === 'warning').length,
      totalCount: state.alerts.length,
    });

    // Subscribe to changes
    const unsubscribe = useEventStats.subscribe((state) => {
      setAlertsData({
        alerts: state.alerts,
        criticalCount: state.alerts.filter((a) => a.type === 'critical').length,
        warningCount: state.alerts.filter((a) => a.type === 'warning').length,
        totalCount: state.alerts.length,
      });
    });

    return unsubscribe;
  }, []);

  return alertsData;
}

// SSR-safe hook for recent events
export function useRecentEvents(): StreamEvent[] {
  const [events, setEvents] = useState<StreamEvent[]>([]);

  useEffect(() => {
    // Get initial state on mount
    setEvents(useEventStats.getState().recentEvents);

    // Subscribe to changes
    const unsubscribe = useEventStats.subscribe((state) => {
      setEvents(state.recentEvents);
    });

    return unsubscribe;
  }, []);

  return events;
}
