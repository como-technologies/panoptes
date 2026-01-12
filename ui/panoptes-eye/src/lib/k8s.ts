import * as k8s from '@kubernetes/client-node';
import type { ArgusWatcher, ArgusWatcherInput } from '@/types/argus';
import type { JanusGuard, JanusGuardInput } from '@/types/janus';
import type { Pod } from '@/types/k8s';

// Kubernetes API configuration
const kc = new k8s.KubeConfig();
let k8sConfigured = false;

// Try kubeconfig first (dev mode), then in-cluster (production)
try {
  kc.loadFromDefault();
  k8sConfigured = true;
  console.log('K8s: Loaded config from default (kubeconfig)');
} catch (defaultErr) {
  console.log('K8s: loadFromDefault failed:', defaultErr instanceof Error ? defaultErr.message : defaultErr);
  try {
    kc.loadFromCluster();
    k8sConfigured = true;
    console.log('K8s: Loaded config from in-cluster ServiceAccount');
  } catch (clusterErr) {
    console.warn('K8s: loadFromCluster failed:', clusterErr instanceof Error ? clusterErr.message : clusterErr);
    console.warn('K8s: No configuration found - API calls will return empty data');
  }
}

if (k8sConfigured) {
  const cluster = kc.getCurrentCluster();
  console.log('K8s: Current cluster:', cluster?.name, cluster?.server);
}

const coreApi = k8sConfigured ? kc.makeApiClient(k8s.CoreV1Api) : null;
const customApi = k8sConfigured ? kc.makeApiClient(k8s.CustomObjectsApi) : null;
const metricsClient = k8sConfigured ? new k8s.Metrics(kc) : null;

// Helper to get APIs with null check
function getCustomApi(): k8s.CustomObjectsApi {
  if (!customApi) throw new K8sError('Kubernetes not configured', 503);
  return customApi;
}

function getCoreApi(): k8s.CoreV1Api {
  if (!coreApi) throw new K8sError('Kubernetes not configured', 503);
  return coreApi;
}

// CRD configuration
const ARGUS_GROUP = 'argus.como-technologies.io';
const ARGUS_VERSION = 'v1';
const ARGUS_PLURAL = 'arguswatchers';

const JANUS_GROUP = 'janus.como-technologies.io';
const JANUS_VERSION = 'v1';
const JANUS_PLURAL = 'janusguards';

// Error wrapper
class K8sError extends Error {
  constructor(
    message: string,
    public statusCode?: number
  ) {
    super(message);
    this.name = 'K8sError';
  }
}

function handleK8sError(error: unknown): never {
  if (error instanceof Error) {
    const k8sErr = error as { response?: { statusCode?: number } };
    throw new K8sError(error.message, k8sErr.response?.statusCode);
  }
  throw new K8sError('Unknown Kubernetes error');
}

// Extract meaningful error message from various error types
function extractErrorMessage(e: unknown): string {
  if (e instanceof Error) {
    // K8s client errors often have response.body.message
    const k8sErr = e as {
      response?: {
        statusCode?: number;
        body?: { message?: string; reason?: string };
      };
      code?: string;
    };
    if (k8sErr.response?.body?.message) {
      return `${k8sErr.response.statusCode || ''} ${k8sErr.response.body.message}`.trim();
    }
    if (k8sErr.response?.body?.reason) {
      return `${k8sErr.response.statusCode || ''} ${k8sErr.response.body.reason}`.trim();
    }
    if (k8sErr.code) {
      return `${k8sErr.code}: ${e.message || 'Connection failed'}`;
    }
    return e.message || e.toString() || 'Unknown error';
  }
  return String(e) || 'Unknown error';
}

// ============= ArgusWatcher Operations =============

export async function listArgusWatchers(namespace?: string): Promise<ArgusWatcher[]> {
  if (!customApi) return [];

  try {
    const api = getCustomApi();
    const response = namespace
      ? await api.listNamespacedCustomObject({
          group: ARGUS_GROUP,
          version: ARGUS_VERSION,
          namespace,
          plural: ARGUS_PLURAL,
        })
      : await api.listClusterCustomObject({
          group: ARGUS_GROUP,
          version: ARGUS_VERSION,
          plural: ARGUS_PLURAL,
        });

    const list = response as { items: ArgusWatcher[] };
    return list.items;
  } catch (error) {
    // For list operations, return empty array on connection/auth errors
    console.warn('Failed to list ArgusWatchers:', error instanceof Error ? error.message : error);
    return [];
  }
}

export async function getArgusWatcher(name: string, namespace: string): Promise<ArgusWatcher> {
  const api = getCustomApi();

  try {
    const response = await api.getNamespacedCustomObject({
      group: ARGUS_GROUP,
      version: ARGUS_VERSION,
      namespace,
      plural: ARGUS_PLURAL,
      name,
    });
    return response as ArgusWatcher;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function createArgusWatcher(input: ArgusWatcherInput): Promise<ArgusWatcher> {
  const api = getCustomApi();

  const watcher: ArgusWatcher = {
    apiVersion: `${ARGUS_GROUP}/${ARGUS_VERSION}`,
    kind: 'ArgusWatcher',
    metadata: {
      name: input.name,
      namespace: input.namespace,
    },
    spec: {
      selector: {
        matchLabels: input.selector,
      },
      subjects: input.subjects,
      paused: input.paused,
      logFormat: input.logFormat,
    },
  };

  try {
    const response = await api.createNamespacedCustomObject({
      group: ARGUS_GROUP,
      version: ARGUS_VERSION,
      namespace: input.namespace,
      plural: ARGUS_PLURAL,
      body: watcher,
    });
    return response as ArgusWatcher;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function updateArgusWatcher(input: ArgusWatcherInput): Promise<ArgusWatcher> {
  const api = getCustomApi();

  try {
    // Get existing watcher to preserve metadata
    const existing = await getArgusWatcher(input.name, input.namespace);

    const watcher: ArgusWatcher = {
      ...existing,
      spec: {
        selector: {
          matchLabels: input.selector,
        },
        subjects: input.subjects,
        paused: input.paused,
        logFormat: input.logFormat,
      },
    };

    const response = await api.replaceNamespacedCustomObject({
      group: ARGUS_GROUP,
      version: ARGUS_VERSION,
      namespace: input.namespace,
      plural: ARGUS_PLURAL,
      name: input.name,
      body: watcher,
    });
    return response as ArgusWatcher;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function deleteArgusWatcher(name: string, namespace: string): Promise<void> {
  const api = getCustomApi();

  try {
    await api.deleteNamespacedCustomObject({
      group: ARGUS_GROUP,
      version: ARGUS_VERSION,
      namespace,
      plural: ARGUS_PLURAL,
      name,
    });
  } catch (error) {
    handleK8sError(error);
  }
}

export async function pauseArgusWatcher(name: string, namespace: string, paused: boolean): Promise<ArgusWatcher> {
  const api = getCustomApi();

  try {
    const response = await api.patchNamespacedCustomObject({
      group: ARGUS_GROUP,
      version: ARGUS_VERSION,
      namespace,
      plural: ARGUS_PLURAL,
      name,
      body: [{ op: 'replace', path: '/spec/paused', value: paused }],
    });
    return response as ArgusWatcher;
  } catch (error) {
    handleK8sError(error);
  }
}

// ============= JanusGuard Operations =============

export async function listJanusGuards(namespace?: string): Promise<JanusGuard[]> {
  if (!customApi) return [];

  try {
    const api = getCustomApi();
    const response = namespace
      ? await api.listNamespacedCustomObject({
          group: JANUS_GROUP,
          version: JANUS_VERSION,
          namespace,
          plural: JANUS_PLURAL,
        })
      : await api.listClusterCustomObject({
          group: JANUS_GROUP,
          version: JANUS_VERSION,
          plural: JANUS_PLURAL,
        });

    const list = response as { items: JanusGuard[] };
    return list.items;
  } catch (error) {
    // For list operations, return empty array on connection/auth errors
    console.warn('Failed to list JanusGuards:', error instanceof Error ? error.message : error);
    return [];
  }
}

export async function getJanusGuard(name: string, namespace: string): Promise<JanusGuard> {
  const api = getCustomApi();

  try {
    const response = await api.getNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace,
      plural: JANUS_PLURAL,
      name,
    });
    return response as JanusGuard;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function createJanusGuard(input: JanusGuardInput): Promise<JanusGuard> {
  const api = getCustomApi();

  const guard: JanusGuard = {
    apiVersion: `${JANUS_GROUP}/${JANUS_VERSION}`,
    kind: 'JanusGuard',
    metadata: {
      name: input.name,
      namespace: input.namespace,
    },
    spec: {
      selector: {
        matchLabels: input.selector,
      },
      subjects: input.subjects,
      paused: input.paused,
      enforcing: input.enforcing,
      logFormat: input.logFormat,
    },
  };

  try {
    const response = await api.createNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace: input.namespace,
      plural: JANUS_PLURAL,
      body: guard,
    });
    return response as JanusGuard;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function updateJanusGuard(input: JanusGuardInput): Promise<JanusGuard> {
  const api = getCustomApi();

  try {
    const existing = await getJanusGuard(input.name, input.namespace);

    const guard: JanusGuard = {
      ...existing,
      spec: {
        selector: {
          matchLabels: input.selector,
        },
        subjects: input.subjects,
        paused: input.paused,
        enforcing: input.enforcing,
        logFormat: input.logFormat,
      },
    };

    const response = await api.replaceNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace: input.namespace,
      plural: JANUS_PLURAL,
      name: input.name,
      body: guard,
    });
    return response as JanusGuard;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function deleteJanusGuard(name: string, namespace: string): Promise<void> {
  const api = getCustomApi();

  try {
    await api.deleteNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace,
      plural: JANUS_PLURAL,
      name,
    });
  } catch (error) {
    handleK8sError(error);
  }
}

export async function pauseJanusGuard(name: string, namespace: string, paused: boolean): Promise<JanusGuard> {
  const api = getCustomApi();

  try {
    const response = await api.patchNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace,
      plural: JANUS_PLURAL,
      name,
      body: [{ op: 'replace', path: '/spec/paused', value: paused }],
    });
    return response as JanusGuard;
  } catch (error) {
    handleK8sError(error);
  }
}

export async function setJanusGuardEnforcing(name: string, namespace: string, enforcing: boolean): Promise<JanusGuard> {
  const api = getCustomApi();

  try {
    const response = await api.patchNamespacedCustomObject({
      group: JANUS_GROUP,
      version: JANUS_VERSION,
      namespace,
      plural: JANUS_PLURAL,
      name,
      body: [{ op: 'replace', path: '/spec/enforcing', value: enforcing }],
    });
    return response as JanusGuard;
  } catch (error) {
    handleK8sError(error);
  }
}

// ============= Pod Operations =============

export async function listPods(labelSelector?: string, namespace?: string): Promise<Pod[]> {
  if (!coreApi) return [];

  try {
    const api = getCoreApi();
    const response = namespace
      ? await api.listNamespacedPod({
          namespace,
          labelSelector,
        })
      : await api.listPodForAllNamespaces({
          labelSelector,
        });

    return response.items as unknown as Pod[];
  } catch (error) {
    // For list operations, return empty array on connection/auth errors
    console.warn('Failed to list Pods:', error instanceof Error ? error.message : error);
    return [];
  }
}

export async function getPod(name: string, namespace: string): Promise<Pod> {
  const api = getCoreApi();

  try {
    const response = await api.readNamespacedPod({ name, namespace });
    return response as unknown as Pod;
  } catch (error) {
    handleK8sError(error);
  }
}

// ============= Daemon Health Operations =============

async function getDaemonHealth(daemonName: string): Promise<{ healthy: number; unhealthy: number; total: number; debug?: string }> {
  if (!coreApi) {
    console.warn(`getDaemonHealth(${daemonName}): coreApi is null - K8s not configured`);
    return { healthy: 0, unhealthy: 0, total: 0, debug: 'coreApi is null' };
  }

  try {
    const labelSelector = `app.kubernetes.io/name=${daemonName}`;
    console.log(`getDaemonHealth(${daemonName}): Querying pods in panoptes-system with label ${labelSelector}`);

    const response = await coreApi.listNamespacedPod({
      namespace: 'panoptes-system',
      labelSelector,
    });

    const pods = response.items;
    const total = pods.length;
    console.log(`getDaemonHealth(${daemonName}): Found ${total} pods`);

    if (total > 0) {
      console.log(`getDaemonHealth(${daemonName}): Pod details:`, pods.map(p => ({
        name: p.metadata?.name,
        phase: p.status?.phase,
        conditions: p.status?.conditions?.map(c => `${c.type}=${c.status}`)
      })));
    }

    const healthy = pods.filter(pod => {
      const isRunning = pod.status?.phase === 'Running';
      const isReady = pod.status?.conditions?.some(
        c => c.type === 'Ready' && c.status === 'True'
      );
      return isRunning && isReady;
    }).length;

    return {
      healthy,
      unhealthy: total - healthy,
      total,
    };
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error);
    console.error(`getDaemonHealth(${daemonName}): Failed -`, errMsg);
    return { healthy: 0, unhealthy: 0, total: 0, debug: errMsg };
  }
}

export interface DaemonHealthInfo {
  healthy: number;
  unhealthy: number;
  total: number;
  debug?: string;
  pods?: Array<{
    name: string;
    phase: string;
    ready: boolean;
  }>;
}

export async function getDaemonHealthInfo(daemonName: string): Promise<DaemonHealthInfo> {
  if (!coreApi) {
    console.warn(`getDaemonHealthInfo(${daemonName}): coreApi is null - K8s not configured`);
    return { healthy: 0, unhealthy: 0, total: 0, debug: 'coreApi is null' };
  }

  try {
    const labelSelector = `app.kubernetes.io/name=${daemonName}`;
    console.log(`getDaemonHealthInfo(${daemonName}): Querying pods in panoptes-system with label ${labelSelector}`);

    const response = await coreApi.listNamespacedPod({
      namespace: 'panoptes-system',
      labelSelector,
    });

    const pods = response.items;
    const total = pods.length;
    console.log(`getDaemonHealthInfo(${daemonName}): Found ${total} pods`);

    const podDetails = pods.map(pod => {
      const isRunning = pod.status?.phase === 'Running';
      const isReady = pod.status?.conditions?.some(
        c => c.type === 'Ready' && c.status === 'True'
      ) ?? false;
      return {
        name: pod.metadata?.name || 'unknown',
        phase: pod.status?.phase || 'Unknown',
        ready: isRunning && isReady,
      };
    });

    const healthy = podDetails.filter(p => p.ready).length;

    return {
      healthy,
      unhealthy: total - healthy,
      total,
      pods: podDetails,
    };
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error);
    console.error(`getDaemonHealthInfo(${daemonName}): Failed -`, errMsg);
    return { healthy: 0, unhealthy: 0, total: 0, debug: errMsg };
  }
}

// ============= Stats Operations =============

export async function getDashboardStats() {
  console.log('getDashboardStats: Starting...');

  const [watchers, guards, argusdHealth, janusdHealth] = await Promise.all([
    listArgusWatchers().catch((err) => {
      console.error('getDashboardStats: listArgusWatchers failed:', err);
      return [];
    }),
    listJanusGuards().catch((err) => {
      console.error('getDashboardStats: listJanusGuards failed:', err);
      return [];
    }),
    getDaemonHealth('argusd'),
    getDaemonHealth('janusd'),
  ]);

  console.log('getDashboardStats: Results -', {
    watcherCount: watchers.length,
    guardCount: guards.length,
    argusdHealth: argusdHealth.total,
    janusdHealth: janusdHealth.total,
  });

  const watcherStats = {
    total: watchers.length,
    active: watchers.filter(w => !w.spec.paused).length,
    paused: watchers.filter(w => w.spec.paused).length,
    error: watchers.filter(w => w.status?.conditions?.some(c => c.type === 'Error' && c.status === 'True')).length,
  };

  const guardStats = {
    total: guards.length,
    enforcing: guards.filter(g => g.spec.enforcing && !g.spec.paused).length,
    audit: guards.filter(g => !g.spec.enforcing && !g.spec.paused).length,
    paused: guards.filter(g => g.spec.paused).length,
  };

  // Calculate event counts from status
  const argusEvents = watchers.reduce((sum, w) => sum + (w.status?.totalEvents || 0), 0);
  const janusEvents = guards.reduce((sum, g) => sum + (g.status?.totalAuditEvents || 0), 0);
  const deniedEvents = guards.reduce((sum, g) => sum + (g.status?.totalDeniedEvents || 0), 0);

  return {
    watchers: watcherStats,
    guards: guardStats,
    events: {
      total24h: argusEvents + janusEvents,
      argusEvents,
      janusEvents,
      deniedEvents,
    },
    alerts: {
      critical: deniedEvents > 0 ? 1 : 0,
      warning: watcherStats.error,
    },
    daemons: {
      argusd: argusdHealth,
      janusd: janusdHealth,
    },
    _debug: {
      k8sConfigured,
      watchersFetched: watchers.length,
      guardsFetched: guards.length,
      argusdDebug: argusdHealth.debug,
      janusdDebug: janusdHealth.debug,
    },
  };
}

// ============= Cluster Info Operations =============

export interface ClusterInfo {
  kubernetesVersion: string;
  platform: string;
  apiServer: string;
  clusterName: string;
  nodeCount: number;
  cloudProvider: string;
  spectroCloudClusterUid?: string;
}

export async function getClusterInfo(): Promise<ClusterInfo | null> {
  console.log('getClusterInfo: Starting, k8sConfigured =', k8sConfigured);
  if (!k8sConfigured) {
    console.log('getClusterInfo: K8s not configured, returning null');
    return null;
  }

  try {
    console.log('getClusterInfo: Fetching version info...');
    const versionApi = kc.makeApiClient(k8s.VersionApi);
    const versionInfo = await versionApi.getCode();
    console.log('getClusterInfo: Got version:', versionInfo.gitVersion);

    const cluster = kc.getCurrentCluster();

    const api = getCoreApi();
    const nodes = await api.listNode({});
    const nodeCount = nodes.items.length;

    // Detect cloud provider from first node
    const firstNode = nodes.items[0];
    const providerID = firstNode?.spec?.providerID || '';
    const labels = (firstNode?.metadata?.labels || {}) as Record<string, string>;

    let cloudProvider = 'Unknown';
    let spectroCloudClusterUid: string | undefined;

    if (labels['spectrocloud.com/cluster-uid']) {
      cloudProvider = 'Spectro Cloud';
      spectroCloudClusterUid = labels['spectrocloud.com/cluster-uid'];
    } else if (providerID.startsWith('gce://')) {
      cloudProvider = 'Google Cloud (GKE)';
    } else if (providerID.startsWith('aws://')) {
      cloudProvider = 'AWS (EKS)';
    } else if (providerID.startsWith('azure://')) {
      cloudProvider = 'Azure (AKS)';
    } else if (providerID.startsWith('kind://')) {
      cloudProvider = 'kind (local)';
    } else if (!providerID) {
      cloudProvider = 'Bare Metal / Unknown';
    }

    return {
      kubernetesVersion: versionInfo.gitVersion,
      platform: versionInfo.platform,
      apiServer: cluster?.server || 'Unknown',
      clusterName: cluster?.name || 'Unknown',
      nodeCount,
      cloudProvider,
      spectroCloudClusterUid,
    };
  } catch (error) {
    console.warn('Failed to get cluster info:', error instanceof Error ? error.message : error);
    return null;
  }
}

// ============= Resource Metrics Operations =============

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
  _debug: {
    k8sConfigured: boolean;
    metricsClientAvailable: boolean;
    error?: string;
  };
}

function parseCpuValue(cpu: string): number {
  if (cpu.endsWith('n')) {
    return parseInt(cpu.slice(0, -1)) / 1000000;
  } else if (cpu.endsWith('u')) {
    return parseInt(cpu.slice(0, -1)) / 1000;
  } else if (cpu.endsWith('m')) {
    return parseInt(cpu.slice(0, -1));
  }
  return parseInt(cpu) * 1000;
}

function parseMemoryValue(memory: string): number {
  const units: Record<string, number> = {
    'Ki': 1024,
    'Mi': 1024 * 1024,
    'Gi': 1024 * 1024 * 1024,
    'K': 1000,
    'M': 1000000,
    'G': 1000000000,
  };

  for (const [suffix, multiplier] of Object.entries(units)) {
    if (memory.endsWith(suffix)) {
      return parseInt(memory.slice(0, -suffix.length)) * multiplier;
    }
  }
  return parseInt(memory);
}

function formatCpu(millicores: number): string {
  if (millicores >= 1000) {
    return `${(millicores / 1000).toFixed(1)} cores`;
  }
  return `${Math.round(millicores)}m`;
}

function formatMemory(bytes: number): string {
  const units = ['B', 'Ki', 'Mi', 'Gi'];
  let value = bytes;
  let unitIndex = 0;

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex++;
  }

  return `${Math.round(value)}${units[unitIndex]}`;
}

export async function getDaemonMetrics(): Promise<DaemonMetrics> {
  const emptyResult: DaemonMetrics = {
    argusd: [],
    janusd: [],
    operators: [],
    totals: {
      argusd: { cpu: 0, memory: 0 },
      janusd: { cpu: 0, memory: 0 },
      operators: { cpu: 0, memory: 0 },
    },
    _debug: {
      k8sConfigured,
      metricsClientAvailable: metricsClient !== null,
    },
  };

  if (!metricsClient) {
    console.warn('getDaemonMetrics: metricsClient is null');
    emptyResult._debug.error = 'metricsClient is null - K8s metrics API not available';
    return emptyResult;
  }

  try {
    const metrics = await metricsClient.getPodMetrics('panoptes-system');

    const categorize = (pods: typeof metrics.items): PodResourceMetrics[] => {
      return pods.map(pod => {
        const containers = pod.containers || [];
        let totalCpu = 0;
        let totalMemory = 0;

        for (const container of containers) {
          const usage = container.usage || {};
          if (usage.cpu) {
            totalCpu += parseCpuValue(usage.cpu);
          }
          if (usage.memory) {
            totalMemory += parseMemoryValue(usage.memory);
          }
        }

        return {
          name: pod.metadata?.name || 'unknown',
          namespace: pod.metadata?.namespace || 'panoptes-system',
          cpu: formatCpu(totalCpu),
          memory: formatMemory(totalMemory),
          cpuMillicores: totalCpu,
          memoryBytes: totalMemory,
        };
      });
    };

    const argusdPods = metrics.items.filter(p => p.metadata?.name?.startsWith('argusd'));
    const janusdPods = metrics.items.filter(p => p.metadata?.name?.startsWith('janusd'));
    const operatorPods = metrics.items.filter(p =>
      p.metadata?.name?.includes('operator') ||
      p.metadata?.name?.includes('argus-operator') ||
      p.metadata?.name?.includes('janus-operator')
    );

    const argusd = categorize(argusdPods);
    const janusd = categorize(janusdPods);
    const operators = categorize(operatorPods);

    const sum = (arr: PodResourceMetrics[]) => ({
      cpu: arr.reduce((acc, p) => acc + p.cpuMillicores, 0),
      memory: arr.reduce((acc, p) => acc + p.memoryBytes, 0),
    });

    return {
      argusd,
      janusd,
      operators,
      totals: {
        argusd: sum(argusd),
        janusd: sum(janusd),
        operators: sum(operators),
      },
      _debug: {
        k8sConfigured,
        metricsClientAvailable: true,
      },
    };
  } catch (error) {
    const errMsg = extractErrorMessage(error);
    console.warn('Failed to get daemon metrics:', errMsg);
    emptyResult._debug.error = errMsg;
    return emptyResult;
  }
}

// ============= Daemon Pod Discovery =============

export interface DaemonPodInfo {
  name: string;
  podIP: string;
  nodeName: string;
  ready: boolean;
}

export interface DaemonEndpoints {
  pods: DaemonPodInfo[];
  endpoints: string[];
}

const DAEMON_PORTS: Record<string, number> = {
  argusd: 50051,
  janusd: 50052,
};

export async function getDaemonPodEndpoints(daemonName: 'argusd' | 'janusd'): Promise<DaemonEndpoints> {
  const port = DAEMON_PORTS[daemonName];
  const emptyResult: DaemonEndpoints = { pods: [], endpoints: [] };

  if (!coreApi) {
    console.warn(`getDaemonPodEndpoints(${daemonName}): coreApi is null`);
    return emptyResult;
  }

  try {
    const labelSelector = `app.kubernetes.io/name=${daemonName}`;
    console.log(`getDaemonPodEndpoints(${daemonName}): Querying pods with label ${labelSelector}`);

    const response = await coreApi.listNamespacedPod({
      namespace: 'panoptes-system',
      labelSelector,
    });

    const pods: DaemonPodInfo[] = [];
    const endpoints: string[] = [];

    for (const pod of response.items) {
      const podIP = pod.status?.podIP;
      const nodeName = pod.spec?.nodeName || 'unknown';
      const isRunning = pod.status?.phase === 'Running';
      const isReady = pod.status?.conditions?.some(
        c => c.type === 'Ready' && c.status === 'True'
      ) ?? false;

      if (podIP && isRunning && isReady) {
        pods.push({
          name: pod.metadata?.name || 'unknown',
          podIP,
          nodeName,
          ready: true,
        });
        endpoints.push(`${podIP}:${port}`);
      } else {
        console.log(`getDaemonPodEndpoints(${daemonName}): Skipping pod ${pod.metadata?.name} - IP: ${podIP}, running: ${isRunning}, ready: ${isReady}`);
      }
    }

    console.log(`getDaemonPodEndpoints(${daemonName}): Found ${endpoints.length} ready endpoints:`, endpoints);
    return { pods, endpoints };
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error);
    console.error(`getDaemonPodEndpoints(${daemonName}): Failed -`, errMsg);
    return emptyResult;
  }
}

// ============= Debug Operations =============

export interface K8sDebugInfo {
  configured: boolean;
  coreApiAvailable: boolean;
  customApiAvailable: boolean;
  metricsClientAvailable: boolean;
  cluster: { name?: string; server?: string } | null;
  configSource: string;
  testResults: {
    listPods: string | null;
    listWatchers: string | null;
    listGuards: string | null;
    listNodes: string | null;
  };
  daemonEndpoints: {
    argusd: DaemonEndpoints;
    janusd: DaemonEndpoints;
  };
  errors: string[];
}

export async function getK8sDebugInfo(): Promise<K8sDebugInfo> {
  const errors: string[] = [];
  const emptyEndpoints: DaemonEndpoints = { pods: [], endpoints: [] };
  const debugInfo: K8sDebugInfo = {
    configured: k8sConfigured,
    coreApiAvailable: coreApi !== null,
    customApiAvailable: customApi !== null,
    metricsClientAvailable: metricsClient !== null,
    cluster: null,
    configSource: k8sConfigured ? 'kubeconfig or in-cluster' : 'none',
    testResults: {
      listPods: null,
      listWatchers: null,
      listGuards: null,
      listNodes: null,
    },
    daemonEndpoints: {
      argusd: emptyEndpoints,
      janusd: emptyEndpoints,
    },
    errors,
  };

  if (!k8sConfigured) {
    errors.push('K8s not configured: loadFromDefault and loadFromCluster both failed');
    return debugInfo;
  }

  const cluster = kc.getCurrentCluster();
  debugInfo.cluster = { name: cluster?.name, server: cluster?.server };

  // Test pod listing
  if (coreApi) {
    try {
      const pods = await coreApi.listNamespacedPod({ namespace: 'panoptes-system' });
      debugInfo.testResults.listPods = `OK: Found ${pods.items.length} pods`;
    } catch (e) {
      const msg = extractErrorMessage(e);
      debugInfo.testResults.listPods = `ERROR: ${msg}`;
      errors.push(`listPods: ${msg}`);
    }

    try {
      const nodes = await coreApi.listNode({});
      debugInfo.testResults.listNodes = `OK: Found ${nodes.items.length} nodes`;
    } catch (e) {
      const msg = extractErrorMessage(e);
      debugInfo.testResults.listNodes = `ERROR: ${msg}`;
      errors.push(`listNodes: ${msg}`);
    }
  } else {
    debugInfo.testResults.listPods = 'SKIPPED: coreApi is null';
    debugInfo.testResults.listNodes = 'SKIPPED: coreApi is null';
  }

  // Test CRD listing
  if (customApi) {
    try {
      const watchers = await customApi.listClusterCustomObject({
        group: ARGUS_GROUP,
        version: ARGUS_VERSION,
        plural: ARGUS_PLURAL,
      });
      const items = (watchers as { items: unknown[] }).items;
      debugInfo.testResults.listWatchers = `OK: Found ${items.length} watchers`;
    } catch (e) {
      const msg = extractErrorMessage(e);
      debugInfo.testResults.listWatchers = `ERROR: ${msg}`;
      errors.push(`listWatchers: ${msg}`);
    }

    try {
      const guards = await customApi.listClusterCustomObject({
        group: JANUS_GROUP,
        version: JANUS_VERSION,
        plural: JANUS_PLURAL,
      });
      const items = (guards as { items: unknown[] }).items;
      debugInfo.testResults.listGuards = `OK: Found ${items.length} guards`;
    } catch (e) {
      const msg = extractErrorMessage(e);
      debugInfo.testResults.listGuards = `ERROR: ${msg}`;
      errors.push(`listGuards: ${msg}`);
    }
  } else {
    debugInfo.testResults.listWatchers = 'SKIPPED: customApi is null';
    debugInfo.testResults.listGuards = 'SKIPPED: customApi is null';
  }

  // Get daemon pod endpoints for multi-node streaming
  const [argusdEndpoints, janusdEndpoints] = await Promise.all([
    getDaemonPodEndpoints('argusd'),
    getDaemonPodEndpoints('janusd'),
  ]);
  debugInfo.daemonEndpoints = { argusd: argusdEndpoints, janusd: janusdEndpoints };

  return debugInfo;
}

// Export error class for handling
export { K8sError };
