'use client';

import { useState, useEffect } from 'react';
import { Settings, Moon, Sun, Monitor, RefreshCw, Download, Check } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Select } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { useDashboardStats } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { useTheme } from '../providers';

const REFRESH_OPTIONS = [
  { value: '10000', label: '10 seconds' },
  { value: '30000', label: '30 seconds' },
  { value: '60000', label: '1 minute' },
  { value: '300000', label: '5 minutes' },
  { value: '0', label: 'Manual only' },
];

interface ClusterInfo {
  kubernetesVersion: string;
  platform: string;
  apiServer: string;
  clusterName: string;
  nodeCount: number;
  cloudProvider: string;
  spectroCloudClusterUid?: string;
}

export default function SettingsPage() {
  const { addToast } = useToast();
  const { data: stats } = useDashboardStats();
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  const [defaultNamespace, setDefaultNamespace] = useState('default');
  const [refreshInterval, setRefreshInterval] = useState('30000');
  const [clusterInfo, setClusterInfo] = useState<ClusterInfo | null>(null);

  useEffect(() => {
    setMounted(true);
    // Load settings from localStorage
    const savedNamespace = localStorage.getItem('panoptes.defaultNamespace');
    const savedInterval = localStorage.getItem('panoptes.refreshInterval');

    if (savedNamespace) setDefaultNamespace(savedNamespace);
    if (savedInterval) setRefreshInterval(savedInterval);

    // Fetch cluster info
    fetch('/api/k8s/cluster')
      .then(res => res.json())
      .then(data => setClusterInfo(data.data))
      .catch(err => console.warn('Failed to fetch cluster info:', err));
  }, []);

  const saveSettings = () => {
    localStorage.setItem('panoptes.defaultNamespace', defaultNamespace);
    localStorage.setItem('panoptes.refreshInterval', refreshInterval);
    addToast({
      variant: 'success',
      title: 'Settings saved',
    });
  };

  const exportAllConfigs = async () => {
    try {
      const [watchersRes, guardsRes] = await Promise.all([
        fetch('/api/k8s/watchers'),
        fetch('/api/k8s/guards'),
      ]);

      const watchers = await watchersRes.json();
      const guards = await guardsRes.json();

      const config = {
        exportedAt: new Date().toISOString(),
        watchers: watchers.data || [],
        guards: guards.data || [],
      };

      const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `panoptes-config-${new Date().toISOString().split('T')[0]}.json`;
      a.click();
      URL.revokeObjectURL(url);

      addToast({
        variant: 'success',
        title: 'Configuration exported',
        description: `Exported ${(watchers.data?.length || 0)} watchers and ${(guards.data?.length || 0)} guards`,
      });
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Export failed',
        description: err instanceof Error ? err.message : 'Failed to export configuration',
      });
    }
  };

  if (!mounted) {
    return null;
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Settings</h1>
        <p className="text-gray-500 dark:text-gray-400">
          Configure your Panoptes dashboard preferences
        </p>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              {theme === 'dark' ? <Moon className="h-5 w-5" /> : <Sun className="h-5 w-5" />}
              Appearance
            </CardTitle>
            <CardDescription>
              Customize the look and feel of the dashboard
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-2">Theme</label>
              <div className="flex gap-2">
                <Button
                  variant={theme === 'light' ? 'primary' : 'outline'}
                  size="sm"
                  onClick={() => setTheme('light')}
                >
                  <Sun className="h-4 w-4 mr-2" />
                  Light
                </Button>
                <Button
                  variant={theme === 'dark' ? 'primary' : 'outline'}
                  size="sm"
                  onClick={() => setTheme('dark')}
                >
                  <Moon className="h-4 w-4 mr-2" />
                  Dark
                </Button>
                <Button
                  variant={theme === 'system' ? 'primary' : 'outline'}
                  size="sm"
                  onClick={() => setTheme('system')}
                >
                  <Monitor className="h-4 w-4 mr-2" />
                  System
                </Button>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <RefreshCw className="h-5 w-5" />
              Data Refresh
            </CardTitle>
            <CardDescription>
              Configure how often data is automatically refreshed
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-2">Refresh Interval</label>
              <Select
                options={REFRESH_OPTIONS}
                value={refreshInterval}
                onChange={(e) => setRefreshInterval(e.target.value)}
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Settings className="h-5 w-5" />
              Defaults
            </CardTitle>
            <CardDescription>
              Set default values for new resources
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-2">Default Namespace</label>
              <Input
                value={defaultNamespace}
                onChange={(e) => setDefaultNamespace(e.target.value)}
                placeholder="default"
              />
            </div>
            <Button onClick={saveSettings}>
              <Check className="h-4 w-4 mr-2" />
              Save Settings
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Download className="h-5 w-5" />
              Export & Import
            </CardTitle>
            <CardDescription>
              Backup or restore your configuration
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <Button variant="outline" onClick={exportAllConfigs} className="w-full">
              <Download className="h-4 w-4 mr-2" />
              Export All Configurations
            </Button>
            <p className="text-xs text-gray-500">
              Export all ArgusWatchers and JanusGuards to a JSON file
            </p>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Connection Status</CardTitle>
          <CardDescription>
            Status of connections to Panoptes daemons
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <div className="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
              <div>
                <p className="text-sm font-medium">API Server</p>
                <p className="text-xs text-gray-500">Kubernetes API</p>
              </div>
              <Badge variant="active">Connected</Badge>
            </div>
            <div className="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
              <div>
                <p className="text-sm font-medium">Argus Daemons</p>
                <p className="text-xs text-gray-500">File watchers</p>
              </div>
              <Badge variant={stats?.daemons.argusd.healthy ? 'active' : 'warning'}>
                {stats?.daemons.argusd.healthy ?? 0}/{stats?.daemons.argusd.total ?? 0}
              </Badge>
            </div>
            <div className="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
              <div>
                <p className="text-sm font-medium">Janus Daemons</p>
                <p className="text-xs text-gray-500">Access guards</p>
              </div>
              <Badge variant={stats?.daemons.janusd.healthy ? 'active' : 'warning'}>
                {stats?.daemons.janusd.healthy ?? 0}/{stats?.daemons.janusd.total ?? 0}
              </Badge>
            </div>
            <div className="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
              <div>
                <p className="text-sm font-medium">Event Stream</p>
                <p className="text-xs text-gray-500">SSE connection</p>
              </div>
              <Badge variant="active">Active</Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>About Panoptes</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-gray-500">Dashboard Version</span>
              <span>1.0.0</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Argus API Version</span>
              <span>argus.como-technologies.io/v1</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Janus API Version</span>
              <span>janus.como-technologies.io/v1</span>
            </div>
            <div className="border-t border-gray-200 dark:border-gray-700 my-3" />
            <div className="flex justify-between">
              <span className="text-gray-500">Kubernetes Version</span>
              <span>{clusterInfo?.kubernetesVersion || 'N/A'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">API Server</span>
              <span className="truncate max-w-[200px] text-right" title={clusterInfo?.apiServer}>
                {clusterInfo?.apiServer || 'N/A'}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Cluster Name</span>
              <span>{clusterInfo?.clusterName || 'N/A'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Cloud Provider</span>
              <span>{clusterInfo?.cloudProvider || 'N/A'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Node Count</span>
              <span>{clusterInfo?.nodeCount ?? 'N/A'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Platform</span>
              <span>{clusterInfo?.platform || 'N/A'}</span>
            </div>
            {clusterInfo?.spectroCloudClusterUid && (
              <div className="flex justify-between">
                <span className="text-gray-500">Spectro Cloud Cluster</span>
                <span className="font-mono text-xs">{clusterInfo.spectroCloudClusterUid}</span>
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
