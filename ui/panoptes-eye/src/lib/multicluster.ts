/**
 * Multi-cluster configuration and utilities for Panoptes.
 *
 * In single-cluster mode, panoptes-eye connects to the local cluster.
 * In multi-cluster mode (future), it can aggregate events from multiple clusters
 * via Prometheus federation or a central event store.
 */

export interface ClusterConfig {
  /** Unique cluster identifier */
  name: string;
  /** Human-readable display name */
  displayName: string;
  /** Environment (production, staging, development) */
  environment?: string;
  /** Region (us-east-1, eu-west-1, etc) */
  region?: string;
  /** Whether this is the current/local cluster */
  isLocal: boolean;
}

export interface MultiClusterConfig {
  /** Whether multi-cluster mode is enabled */
  enabled: boolean;
  /** Current cluster (always present) */
  currentCluster: ClusterConfig;
  /** All known clusters (for future multi-cluster UI) */
  clusters: ClusterConfig[];
  /** Prometheus federation URL (if configured) */
  prometheusUrl?: string;
}

/**
 * Get multi-cluster configuration from environment.
 *
 * Environment variables:
 * - PANOPTES_CLUSTER_NAME: Current cluster name
 * - PANOPTES_MULTICLUSTER: Enable multi-cluster mode (true/false)
 * - PROMETHEUS_URL: URL for federated Prometheus queries
 */
export function getMultiClusterConfig(): MultiClusterConfig {
  const clusterName = process.env.PANOPTES_CLUSTER_NAME ||
                      process.env.NEXT_PUBLIC_CLUSTER_NAME ||
                      '';
  const environment = process.env.PANOPTES_CLUSTER_ENVIRONMENT ||
                      process.env.NEXT_PUBLIC_CLUSTER_ENVIRONMENT ||
                      '';
  const region = process.env.PANOPTES_CLUSTER_REGION ||
                 process.env.NEXT_PUBLIC_CLUSTER_REGION ||
                 '';
  const multiClusterEnabled = process.env.PANOPTES_MULTICLUSTER === 'true' ||
                              process.env.NEXT_PUBLIC_MULTICLUSTER === 'true';
  const prometheusUrl = process.env.PROMETHEUS_URL ||
                        process.env.NEXT_PUBLIC_PROMETHEUS_URL;

  const currentCluster: ClusterConfig = {
    name: clusterName || 'local',
    displayName: clusterName || 'Local Cluster',
    environment: environment || undefined,
    region: region || undefined,
    isLocal: true,
  };

  return {
    enabled: multiClusterEnabled,
    currentCluster,
    clusters: [currentCluster], // Future: populate from discovery
    prometheusUrl,
  };
}

/**
 * Get a display-friendly cluster label.
 * Returns environment-region format if available, otherwise just the name.
 */
export function getClusterLabel(cluster: ClusterConfig): string {
  if (cluster.environment && cluster.region) {
    return `${cluster.environment}-${cluster.region}`;
  }
  if (cluster.environment) {
    return cluster.environment;
  }
  return cluster.displayName;
}

/**
 * Get cluster badge color based on environment.
 */
export function getClusterColor(environment?: string): string {
  switch (environment?.toLowerCase()) {
    case 'production':
    case 'prod':
      return 'red';
    case 'staging':
    case 'stage':
      return 'yellow';
    case 'development':
    case 'dev':
      return 'green';
    default:
      return 'blue';
  }
}

/**
 * Format cluster name for display.
 * Handles empty cluster names gracefully.
 */
export function formatClusterName(clusterName: string | undefined): string {
  if (!clusterName || clusterName.trim() === '') {
    return 'Local';
  }
  return clusterName;
}
