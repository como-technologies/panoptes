// Common Kubernetes types

export interface ObjectMeta {
  name: string;
  namespace: string;
  uid?: string;
  creationTimestamp?: string;
  generation?: number;
  resourceVersion?: string;
  labels?: Record<string, string>;
  annotations?: Record<string, string>;
}

export interface LabelSelector {
  matchLabels?: Record<string, string>;
  matchExpressions?: LabelSelectorRequirement[];
}

export interface LabelSelectorRequirement {
  key: string;
  operator: 'In' | 'NotIn' | 'Exists' | 'DoesNotExist';
  values?: string[];
}

export interface Pod {
  metadata: ObjectMeta;
  spec: {
    nodeName?: string;
    containers: Container[];
    initContainers?: Container[];
  };
  status: {
    phase: 'Pending' | 'Running' | 'Succeeded' | 'Failed' | 'Unknown';
    containerStatuses?: ContainerStatus[];
    podIP?: string;
    hostIP?: string;
  };
}

export interface Container {
  name: string;
  image: string;
  ports?: ContainerPort[];
  env?: EnvVar[];
  volumeMounts?: VolumeMount[];
}

export interface ContainerPort {
  name?: string;
  containerPort: number;
  protocol?: 'TCP' | 'UDP' | 'SCTP';
}

export interface EnvVar {
  name: string;
  value?: string;
}

export interface VolumeMount {
  name: string;
  mountPath: string;
  readOnly?: boolean;
}

export interface ContainerStatus {
  name: string;
  ready: boolean;
  restartCount: number;
  state: {
    running?: { startedAt: string };
    waiting?: { reason: string; message?: string };
    terminated?: { exitCode: number; reason: string };
  };
  containerID?: string;
}

export interface Node {
  metadata: ObjectMeta;
  status: {
    conditions: NodeCondition[];
    addresses: NodeAddress[];
  };
}

export interface NodeCondition {
  type: string;
  status: 'True' | 'False' | 'Unknown';
  reason?: string;
  message?: string;
}

export interface NodeAddress {
  type: 'Hostname' | 'InternalIP' | 'ExternalIP';
  address: string;
}

// File system types for explorer
export interface FileEntry {
  name: string;
  path: string;
  type: 'file' | 'directory' | 'symlink';
  size?: number;
  mode?: string;
  modTime?: string;
  children?: FileEntry[];
}
