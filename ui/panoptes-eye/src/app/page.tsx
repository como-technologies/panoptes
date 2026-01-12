'use client';

import Link from 'next/link';
import { Eye, FileSearch, Shield, Activity, AlertTriangle, Plus, RefreshCw, Cpu, HardDrive } from 'lucide-react';
import { getJanusMode } from '@/types/janus';
import { useDashboardStats, useWatchers, useGuards, useDaemonMetrics } from '@/hooks/useK8s';
import { useEventCounts, useAlerts } from '@/stores/eventStats';
import { StatCard, Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge, StatusBadge, ModeBadge } from '@/components/ui/badge';
import { Skeleton, SkeletonCard } from '@/components/ui/skeleton';

function formatNumber(num: number): string {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
  if (num >= 1000) return `${(num / 1000).toFixed(1)}k`;
  return num.toString();
}

function StatsGrid() {
  const { data: stats, isLoading, error, refetch } = useDashboardStats();
  const liveEventCounts = useEventCounts();
  const liveAlerts = useAlerts();

  if (isLoading) {
    return (
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {[1, 2, 3, 4].map((i) => (
          <SkeletonCard key={i} />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <Card>
        <CardContent className="p-6 text-center">
          <p className="text-red-500 mb-4">Failed to load dashboard stats</p>
          <Button variant="outline" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  // Merge CRD-based stats with live event counts from Zustand store
  const totalEvents = (stats?.events.total24h ?? 0) + liveEventCounts.total;
  const totalAlerts =
    (stats?.alerts.critical ?? 0) +
    (stats?.alerts.warning ?? 0) +
    liveAlerts.criticalCount +
    liveAlerts.warningCount;

  return (
    <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
      <StatCard
        title="Active Watchers"
        value={stats?.watchers.active ?? 0}
        description="ArgusWatcher resources"
        icon={<FileSearch className="h-6 w-6 text-blue-500" />}
      />
      <StatCard
        title="Active Guards"
        value={stats?.guards.total ?? 0}
        description="JanusGuard resources"
        icon={<Shield className="h-6 w-6 text-purple-500" />}
      />
      <StatCard
        title="Events (24h)"
        value={formatNumber(totalEvents)}
        description={`${liveEventCounts.total > 0 ? `+${liveEventCounts.total} live` : 'File system events'}`}
        icon={<Activity className="h-6 w-6 text-green-500" />}
      />
      <StatCard
        title="Alerts"
        value={totalAlerts}
        description={liveAlerts.criticalCount > 0 ? `${liveAlerts.criticalCount} critical` : 'Requiring attention'}
        icon={<AlertTriangle className={`h-6 w-6 ${liveAlerts.criticalCount > 0 ? 'text-red-500 animate-pulse' : 'text-red-500'}`} />}
      />
    </div>
  );
}

function QuickActions() {
  return (
    <div className="flex flex-wrap gap-3">
      <Link href="/watchers/new">
        <Button>
          <Plus className="h-4 w-4 mr-2" />
          New Watcher
        </Button>
      </Link>
      <Link href="/guards/new">
        <Button variant="secondary">
          <Plus className="h-4 w-4 mr-2" />
          New Guard
        </Button>
      </Link>
      <Link href="/events">
        <Button variant="outline">
          <Activity className="h-4 w-4 mr-2" />
          View Events
        </Button>
      </Link>
    </div>
  );
}

function RecentWatchers() {
  const { data: watchers, isLoading } = useWatchers();

  if (isLoading) {
    return (
      <Card>
        <div className="border-b p-4">
          <Skeleton className="h-5 w-32" />
        </div>
        <div className="p-4 space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="flex justify-between">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-4 w-16" />
            </div>
          ))}
        </div>
      </Card>
    );
  }

  const recentWatchers = (watchers || []).slice(0, 5);

  return (
    <Card>
      <div className="border-b p-4 flex items-center justify-between">
        <h3 className="font-semibold">Recent Watchers</h3>
        <Link href="/watchers" className="text-sm text-blue-500 hover:underline">
          View all
        </Link>
      </div>
      <div className="divide-y dark:divide-gray-700">
        {recentWatchers.length === 0 ? (
          <div className="p-6 text-center text-gray-500">
            <FileSearch className="h-8 w-8 mx-auto mb-2 opacity-50" />
            <p>No watchers yet</p>
            <Link href="/watchers/new">
              <Button variant="ghost" size="sm" className="mt-2">
                Create one
              </Button>
            </Link>
          </div>
        ) : (
          recentWatchers.map((watcher) => (
            <Link
              key={`${watcher.metadata.namespace}/${watcher.metadata.name}`}
              href={`/watchers/${watcher.metadata.name}?namespace=${watcher.metadata.namespace}`}
              className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
            >
              <div>
                <p className="font-medium">{watcher.metadata.name}</p>
                <p className="text-sm text-gray-500">{watcher.metadata.namespace}</p>
              </div>
              <StatusBadge status={watcher.spec.paused ? 'paused' : 'active'} />
            </Link>
          ))
        )}
      </div>
    </Card>
  );
}

function RecentGuards() {
  const { data: guards, isLoading } = useGuards();

  if (isLoading) {
    return (
      <Card>
        <div className="border-b p-4">
          <Skeleton className="h-5 w-32" />
        </div>
        <div className="p-4 space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="flex justify-between">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-4 w-16" />
            </div>
          ))}
        </div>
      </Card>
    );
  }

  const recentGuards = (guards || []).slice(0, 5);

  return (
    <Card>
      <div className="border-b p-4 flex items-center justify-between">
        <h3 className="font-semibold">Recent Guards</h3>
        <Link href="/guards" className="text-sm text-blue-500 hover:underline">
          View all
        </Link>
      </div>
      <div className="divide-y dark:divide-gray-700">
        {recentGuards.length === 0 ? (
          <div className="p-6 text-center text-gray-500">
            <Shield className="h-8 w-8 mx-auto mb-2 opacity-50" />
            <p>No guards yet</p>
            <Link href="/guards/new">
              <Button variant="ghost" size="sm" className="mt-2">
                Create one
              </Button>
            </Link>
          </div>
        ) : (
          recentGuards.map((guard) => (
            <Link
              key={`${guard.metadata.namespace}/${guard.metadata.name}`}
              href={`/guards/${guard.metadata.name}?namespace=${guard.metadata.namespace}`}
              className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
            >
              <div>
                <p className="font-medium">{guard.metadata.name}</p>
                <p className="text-sm text-gray-500">{guard.metadata.namespace}</p>
              </div>
              <ModeBadge mode={getJanusMode(guard)} />
            </Link>
          ))
        )}
      </div>
    </Card>
  );
}

function DaemonHealth() {
  const { data: stats, isLoading } = useDashboardStats();

  if (isLoading) {
    return (
      <Card>
        <CardContent className="p-6">
          <Skeleton className="h-5 w-32 mb-4" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
          </div>
        </CardContent>
      </Card>
    );
  }

  const argusdHealthy = stats?.daemons.argusd.healthy ?? 0;
  const argusdTotal = stats?.daemons.argusd.total ?? 0;
  const janusdHealthy = stats?.daemons.janusd.healthy ?? 0;
  const janusdTotal = stats?.daemons.janusd.total ?? 0;

  return (
    <Card>
      <CardContent className="p-6">
        <h3 className="font-semibold mb-4">Daemon Status</h3>
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Eye className="h-4 w-4 text-blue-500" />
              <span className="text-sm">Argus Daemon</span>
            </div>
            <Badge variant={argusdHealthy === argusdTotal && argusdTotal > 0 ? 'active' : argusdTotal === 0 ? 'default' : 'warning'}>
              {argusdTotal > 0 ? `${argusdHealthy}/${argusdTotal}` : 'N/A'}
            </Badge>
          </div>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Shield className="h-4 w-4 text-purple-500" />
              <span className="text-sm">Janus Daemon</span>
            </div>
            <Badge variant={janusdHealthy === janusdTotal && janusdTotal > 0 ? 'active' : janusdTotal === 0 ? 'default' : 'warning'}>
              {janusdTotal > 0 ? `${janusdHealthy}/${janusdTotal}` : 'N/A'}
            </Badge>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'Ki', 'Mi', 'Gi'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex++;
  }
  return `${Math.round(value)}${units[unitIndex]}`;
}

function formatCpu(millicores: number): string {
  if (millicores >= 1000) {
    return `${(millicores / 1000).toFixed(1)} cores`;
  }
  return `${Math.round(millicores)}m`;
}

function ResourceMetrics() {
  const { data: metrics, isLoading } = useDaemonMetrics();

  if (isLoading) {
    return (
      <Card>
        <CardContent className="p-6">
          <Skeleton className="h-5 w-32 mb-4" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
          </div>
        </CardContent>
      </Card>
    );
  }

  const totals = metrics?.totals;
  const hasMetrics = totals && (totals.argusd.cpu > 0 || totals.janusd.cpu > 0 || totals.operators.cpu > 0);

  return (
    <Card>
      <CardContent className="p-6">
        <h3 className="font-semibold mb-4">Resource Usage</h3>
        {!hasMetrics ? (
          <p className="text-sm text-gray-500">No metrics available</p>
        ) : (
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Eye className="h-4 w-4 text-blue-500" />
                <span className="text-sm">argusd</span>
              </div>
              <div className="flex items-center gap-3 text-sm">
                <span className="flex items-center gap-1">
                  <Cpu className="h-3 w-3 text-gray-400" />
                  {formatCpu(totals.argusd.cpu)}
                </span>
                <span className="flex items-center gap-1">
                  <HardDrive className="h-3 w-3 text-gray-400" />
                  {formatBytes(totals.argusd.memory)}
                </span>
              </div>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Shield className="h-4 w-4 text-purple-500" />
                <span className="text-sm">janusd</span>
              </div>
              <div className="flex items-center gap-3 text-sm">
                <span className="flex items-center gap-1">
                  <Cpu className="h-3 w-3 text-gray-400" />
                  {formatCpu(totals.janusd.cpu)}
                </span>
                <span className="flex items-center gap-1">
                  <HardDrive className="h-3 w-3 text-gray-400" />
                  {formatBytes(totals.janusd.memory)}
                </span>
              </div>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-green-500" />
                <span className="text-sm">Operators</span>
              </div>
              <div className="flex items-center gap-3 text-sm">
                <span className="flex items-center gap-1">
                  <Cpu className="h-3 w-3 text-gray-400" />
                  {formatCpu(totals.operators.cpu)}
                </span>
                <span className="flex items-center gap-1">
                  <HardDrive className="h-3 w-3 text-gray-400" />
                  {formatBytes(totals.operators.memory)}
                </span>
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

export default function DashboardPage() {
  return (
    <div className="space-y-8">
      <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Panoptes Dashboard</h1>
          <p className="text-gray-500 dark:text-gray-400">
            All-seeing security monitoring for your Kubernetes clusters
          </p>
        </div>
        <QuickActions />
      </div>

      <StatsGrid />

      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-4">
        <RecentWatchers />
        <RecentGuards />
        <DaemonHealth />
        <ResourceMetrics />
      </div>
    </div>
  );
}
