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

interface EventStatsState {
  // Counts
  argusEvents: number;
  janusEvents: number;
  deniedEvents: number;
  allowedEvents: number;
  auditEvents: number;

  // Alerts
  alerts: Alert[];
}

interface EventStatsActions {
  incrementArgus: () => void;
  incrementJanus: (action: 'allowed' | 'denied' | 'audit') => void;
  addAlert: (alert: Omit<Alert, 'id' | 'timestamp'>) => void;
  dismissAlert: (id: string) => void;
  clearAlerts: () => void;
  resetStats: () => void;
}

type EventStatsStore = EventStatsState & EventStatsActions;

const MAX_ALERTS = 100;

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
