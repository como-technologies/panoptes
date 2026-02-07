// TypeScript types matching the argus.proto and janus.proto definitions

// Argus (inotify) event types
export enum InotifyEvent {
  UNSPECIFIED = 0,
  ACCESS = 1,
  ATTRIB = 2,
  CLOSE_WRITE = 3,
  CLOSE_NOWRITE = 4,
  CREATE = 5,
  DELETE = 6,
  DELETE_SELF = 7,
  MODIFY = 8,
  MOVE_SELF = 9,
  MOVED_FROM = 10,
  MOVED_TO = 11,
  OPEN = 12,
  ALL = 99,
}

// Janus (fanotify) event types
export enum FanotifyEvent {
  UNSPECIFIED = 0,
  ACCESS = 1,
  OPEN = 2,
  OPEN_EXEC = 3,
  CLOSE_WRITE = 4,
  CLOSE = 5,
  ALL = 99,
}

// Janus access response
export enum AccessResponse {
  UNSPECIFIED = 0,
  ALLOW = 1,
  DENY = 2,
  AUDIT = 3,
}

// Argus FileEvent from proto
// V2 adds optional processInfo field (empty until eBPF integration)
export interface FileEvent {
  timestamp: { seconds: number; nanos: number } | string;
  watcherName: string;
  namespace: string;
  nodeName: string;
  podName: string;
  containerId: string;
  eventType: InotifyEvent;
  path: string;
  filename: string;
  isDirectory: boolean;
  inode: number;
  tags: Record<string, string>;
  // V2 field - will be empty for inotify until eBPF/audit integration
  processInfo?: ProcessInfo;
}

// ProcessInfo from proto (v1 + v2 extensions)
// V2 adds ppid, cmdline, and cwd fields
export interface ProcessInfo {
  // V1 fields
  pid: number;
  tid: number;
  uid: number;
  gid: number;
  comm: string;
  exe: string;
  // V2 fields (Rust daemon only)
  ppid?: number;
  cmdline?: string[];
  cwd?: string;
}

// Helper to check if ProcessInfo has v2 extended fields
export function hasV2ProcessInfo(info: ProcessInfo | undefined): boolean {
  if (!info) return false;
  return info.ppid !== undefined || (info.cmdline?.length ?? 0) > 0 || !!info.cwd;
}

// Janus AccessEvent from proto
export interface AccessEvent {
  timestamp: { seconds: number; nanos: number } | string;
  guardName: string;
  namespace: string;
  nodeName: string;
  podName: string;
  containerId: string;
  eventType: FanotifyEvent;
  path: string;
  response: AccessResponse;
  processInfo?: ProcessInfo;
  isDirectory: boolean;
  tags: Record<string, string>;
  auditLogged: boolean;
}

// Unified event type for UI
export interface UnifiedEvent {
  id: string;
  timestamp: string;
  source: 'argus' | 'janus';
  /** The name of the ArgusWatcher or JanusGuard that generated this event */
  resourceName: string;
  eventType: string;
  path: string;
  podName: string;
  nodeName: string;
  namespace: string;
  action: 'allowed' | 'denied' | 'audit' | 'detected';
  containerId?: string;
  processInfo?: ProcessInfo;
  tags?: Record<string, string>;
}

// Convert InotifyEvent enum to string
export function inotifyEventToString(event: InotifyEvent): string {
  const map: Record<InotifyEvent, string> = {
    [InotifyEvent.UNSPECIFIED]: 'unknown',
    [InotifyEvent.ACCESS]: 'access',
    [InotifyEvent.ATTRIB]: 'attrib',
    [InotifyEvent.CLOSE_WRITE]: 'close_write',
    [InotifyEvent.CLOSE_NOWRITE]: 'close_nowrite',
    [InotifyEvent.CREATE]: 'create',
    [InotifyEvent.DELETE]: 'delete',
    [InotifyEvent.DELETE_SELF]: 'delete_self',
    [InotifyEvent.MODIFY]: 'modify',
    [InotifyEvent.MOVE_SELF]: 'move_self',
    [InotifyEvent.MOVED_FROM]: 'moved_from',
    [InotifyEvent.MOVED_TO]: 'moved_to',
    [InotifyEvent.OPEN]: 'open',
    [InotifyEvent.ALL]: 'all',
  };
  return map[event] || 'unknown';
}

// Convert FanotifyEvent enum to string
export function fanotifyEventToString(event: FanotifyEvent): string {
  const map: Record<FanotifyEvent, string> = {
    [FanotifyEvent.UNSPECIFIED]: 'unknown',
    [FanotifyEvent.ACCESS]: 'access',
    [FanotifyEvent.OPEN]: 'open',
    [FanotifyEvent.OPEN_EXEC]: 'open_exec',
    [FanotifyEvent.CLOSE_WRITE]: 'close_write',
    [FanotifyEvent.CLOSE]: 'close',
    [FanotifyEvent.ALL]: 'all',
  };
  return map[event] || 'unknown';
}

// Convert AccessResponse enum to UI action
export function accessResponseToAction(response: AccessResponse): 'allowed' | 'denied' | 'audit' {
  switch (response) {
    case AccessResponse.ALLOW:
      return 'allowed';
    case AccessResponse.DENY:
      return 'denied';
    case AccessResponse.AUDIT:
      return 'audit';
    default:
      return 'allowed';
  }
}

// Convert FileEvent to UnifiedEvent
export function fileEventToUnified(event: FileEvent, id: string): UnifiedEvent {
  const timestamp = typeof event.timestamp === 'string'
    ? event.timestamp
    : new Date(event.timestamp.seconds * 1000 + event.timestamp.nanos / 1000000).toISOString();

  return {
    id,
    timestamp,
    source: 'argus',
    resourceName: event.watcherName,
    eventType: inotifyEventToString(event.eventType),
    path: event.path,
    podName: event.podName,
    nodeName: event.nodeName,
    namespace: event.namespace,
    action: 'detected',
    containerId: event.containerId,
    processInfo: event.processInfo, // V2 field (empty for inotify until eBPF)
    tags: event.tags,
  };
}

// Convert AccessEvent to UnifiedEvent
export function accessEventToUnified(event: AccessEvent, id: string): UnifiedEvent {
  const timestamp = typeof event.timestamp === 'string'
    ? event.timestamp
    : new Date(event.timestamp.seconds * 1000 + event.timestamp.nanos / 1000000).toISOString();

  return {
    id,
    timestamp,
    source: 'janus',
    resourceName: event.guardName,
    eventType: fanotifyEventToString(event.eventType),
    path: event.path,
    podName: event.podName,
    nodeName: event.nodeName,
    namespace: event.namespace,
    action: accessResponseToAction(event.response),
    containerId: event.containerId,
    processInfo: event.processInfo,
    tags: event.tags,
  };
}
