// Re-export all types
export * from './k8s';
export * from './argus';
export * from './janus';
export * from './compliance';

// Dashboard statistics
export interface DashboardStats {
  watchers: {
    total: number;
    active: number;
    paused: number;
    error: number;
  };
  guards: {
    total: number;
    enforcing: number;
    audit: number;
    paused: number;
  };
  events: {
    total24h: number;
    argusEvents: number;
    janusEvents: number;
    deniedEvents: number;
  };
  alerts: {
    critical: number;
    warning: number;
  };
  daemons: {
    argusd: DaemonStatus;
    janusd: DaemonStatus;
  };
}

export interface DaemonStatus {
  healthy: number;
  unhealthy: number;
  total: number;
}

export interface PodResourceMetrics {
  name: string;
  namespace: string;
  cpu: string;
  memory: string;
  cpuMillicores: number;
  memoryBytes: number;
}

export interface DaemonMetrics {
  argusd: PodResourceMetrics[];
  janusd: PodResourceMetrics[];
  operators: PodResourceMetrics[];
  totals: {
    argusd: { cpu: number; memory: number };
    janusd: { cpu: number; memory: number };
    operators: { cpu: number; memory: number };
  };
}

// Combined event type for unified event stream
export type UnifiedEvent =
  | { type: 'argus'; event: import('./argus').ArgusEvent }
  | { type: 'janus'; event: import('./janus').JanusAuditEvent };

// API response wrappers
export interface ApiResponse<T> {
  data: T;
  error?: string;
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}
