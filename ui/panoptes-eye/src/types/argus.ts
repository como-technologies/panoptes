// Argus CRD types - File Integrity Monitoring

import type { ObjectMeta, LabelSelector } from './k8s';

export interface ArgusWatcher {
  apiVersion: 'argus.como-technologies.io/v1';
  kind: 'ArgusWatcher';
  metadata: ObjectMeta;
  spec: ArgusWatcherSpec;
  status?: ArgusWatcherStatus;
}

export interface ArgusWatcherSpec {
  selector: LabelSelector;
  subjects: ArgusSubject[];
  paused?: boolean;
  logFormat?: string;
}

export interface ArgusSubject {
  paths: string[];
  events: ArgusEventType[];
  recursive?: boolean;
  maxDepth?: number;
  ignores?: string[];
  onlyDir?: boolean;
  followMove?: boolean;
  tags?: Record<string, string>;
}

export type ArgusEventType =
  | 'access'
  | 'attrib'
  | 'close_write'
  | 'close_nowrite'
  | 'close'
  | 'create'
  | 'delete'
  | 'delete_self'
  | 'modify'
  | 'move_self'
  | 'moved_from'
  | 'moved_to'
  | 'move'
  | 'open'
  | 'all';

export interface ArgusWatcherStatus {
  observedGeneration?: number;
  conditions?: ArgusCondition[];
  observablePods?: number;
  watchedPods?: number;
  totalEvents?: number;
  lastEventTime?: string;
}

export interface ArgusCondition {
  type: 'Ready' | 'Degraded' | 'Error';
  status: 'True' | 'False' | 'Unknown';
  reason?: string;
  message?: string;
  lastTransitionTime?: string;
}

// Event from argusd daemon
export interface ArgusEvent {
  id: string;
  timestamp: string;
  watcherName: string;
  watcherNamespace: string;
  nodeName: string;
  podName: string;
  containerName?: string;
  eventType: ArgusEventType;
  path: string;
  fileName: string;
  isDirectory: boolean;
  tags?: Record<string, string>;
}

// For creating/updating watchers
export interface ArgusWatcherInput {
  name: string;
  namespace: string;
  selector: Record<string, string>;
  subjects: ArgusSubject[];
  paused?: boolean;
  logFormat?: string;
}
