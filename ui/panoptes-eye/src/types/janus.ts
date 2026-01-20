// Janus CRD types - File Access Auditing

import type { ObjectMeta, LabelSelector } from './k8s';

export interface JanusGuard {
  apiVersion: 'janus.como-technologies.io/v1';
  kind: 'JanusGuard';
  metadata: ObjectMeta;
  spec: JanusGuardSpec;
  status?: JanusGuardStatus;
}

export interface JanusGuardSpec {
  selector: LabelSelector;
  subjects: JanusSubject[];
  paused?: boolean;
  enforcing?: boolean;
  logFormat?: string;
}

export interface JanusSubject {
  allow?: string[];
  deny?: string[];
  events: JanusEventType[];
  audit?: boolean;
  tags?: Record<string, string>;
}

export type JanusEventType =
  | 'access'
  | 'open'
  | 'open_exec'
  | 'open_write'
  | 'open_read'
  | 'close'
  | 'close_write'
  | 'close_nowrite';

export interface JanusGuardStatus {
  observedGeneration?: number;
  conditions?: JanusCondition[];
  observablePods?: number;
  guardedPods?: number;
  totalDeniedEvents?: number;
  totalAuditEvents?: number;
  lastEventTime?: string;
}

export interface JanusCondition {
  type: 'Ready' | 'Degraded' | 'Error';
  status: 'True' | 'False' | 'Unknown';
  reason?: string;
  message?: string;
  lastTransitionTime?: string;
}

// Audit event from janusd daemon
export interface JanusAuditEvent {
  id: string;
  timestamp: string;
  guardName: string;
  guardNamespace: string;
  nodeName: string;
  podName: string;
  containerName?: string;
  eventType: JanusEventType;
  path: string;
  action: 'allowed' | 'denied' | 'audit';
  processName?: string;
  processId?: number;
  tags?: Record<string, string>;
  /** Cluster name for multi-cluster deployments */
  clusterName?: string;
}

// For creating/updating guards
export interface JanusGuardInput {
  name: string;
  namespace: string;
  selector: Record<string, string>;
  subjects: JanusSubject[];
  paused?: boolean;
  enforcing?: boolean;
  logFormat?: string;
}

// Mode helpers
export type JanusMode = 'paused' | 'audit' | 'enforcing';

export function getJanusMode(guard: JanusGuard): JanusMode {
  if (guard.spec.paused) return 'paused';
  if (guard.spec.enforcing) return 'enforcing';
  return 'audit';
}
