'use client';

import { useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Edit, Trash2, Play, Pause, RefreshCw, Copy, FileText } from 'lucide-react';
import { useState, use } from 'react';
import { useWatcher, useDeleteWatcher, usePauseWatcher } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge, StatusBadge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ConfirmDialog } from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { useRouter } from 'next/navigation';
import YAML from 'yaml';

interface PageProps {
  params: Promise<{ name: string }>;
}

export default function WatcherDetailPage({ params }: PageProps) {
  const { name } = use(params);
  const searchParams = useSearchParams();
  const namespace = searchParams.get('namespace') || 'default';
  const router = useRouter();
  const [showDelete, setShowDelete] = useState(false);
  const [activeTab, setActiveTab] = useState('overview');

  const { data: watcher, isLoading, error, refetch } = useWatcher(name, namespace);
  const deleteWatcher = useDeleteWatcher();
  const pauseWatcher = usePauseWatcher();
  const { addToast } = useToast();

  const handlePause = async () => {
    if (!watcher) return;
    const isPaused = watcher.spec.paused;
    try {
      await pauseWatcher.mutateAsync({
        name: watcher.metadata.name,
        namespace: watcher.metadata.namespace,
        paused: !isPaused,
      });
      addToast({
        variant: 'success',
        title: isPaused ? 'Watcher resumed' : 'Watcher paused',
      });
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Action failed',
        description: err instanceof Error ? err.message : 'Failed to update watcher',
      });
    }
  };

  const handleDelete = async () => {
    if (!watcher) return;
    try {
      await deleteWatcher.mutateAsync({
        name: watcher.metadata.name,
        namespace: watcher.metadata.namespace,
      });
      addToast({
        variant: 'success',
        title: 'Watcher deleted',
      });
      router.push('/watchers');
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Delete failed',
        description: err instanceof Error ? err.message : 'Failed to delete watcher',
      });
    }
  };

  const copyYaml = () => {
    if (!watcher) return;
    const yamlStr = YAML.stringify(watcher);
    navigator.clipboard.writeText(yamlStr);
    addToast({
      variant: 'success',
      title: 'Copied to clipboard',
    });
  };

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Skeleton className="h-10 w-10" />
          <div>
            <Skeleton className="h-8 w-48 mb-2" />
            <Skeleton className="h-4 w-32" />
          </div>
        </div>
        <Card>
          <CardContent className="p-6">
            <div className="space-y-4">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-4 w-1/2" />
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (error || !watcher) {
    return (
      <div className="space-y-6">
        <Link href="/watchers" className="inline-flex items-center text-sm text-blue-600 hover:underline">
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to Watchers
        </Link>
        <Card className="p-8 text-center">
          <p className="text-red-500 mb-4">Failed to load watcher</p>
          <Button variant="outline" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Retry
          </Button>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <Link href="/watchers" className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2">
            <ArrowLeft className="h-4 w-4 mr-1" />
            Back to Watchers
          </Link>
          <h1 className="text-3xl font-bold tracking-tight">{watcher.metadata.name}</h1>
          <div className="flex items-center gap-2 mt-2">
            <Badge variant="default">{watcher.metadata.namespace}</Badge>
            <StatusBadge status={watcher.spec.paused ? 'paused' : 'active'} />
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={handlePause} disabled={pauseWatcher.isPending}>
            {watcher.spec.paused ? (
              <>
                <Play className="h-4 w-4 mr-2" />
                Resume
              </>
            ) : (
              <>
                <Pause className="h-4 w-4 mr-2" />
                Pause
              </>
            )}
          </Button>
          <Link href={`/watchers/${name}/edit?namespace=${namespace}`}>
            <Button variant="secondary">
              <Edit className="h-4 w-4 mr-2" />
              Edit
            </Button>
          </Link>
          <Button variant="destructive" onClick={() => setShowDelete(true)}>
            <Trash2 className="h-4 w-4 mr-2" />
            Delete
          </Button>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="subjects">Subjects</TabsTrigger>
          <TabsTrigger value="yaml">YAML</TabsTrigger>
        </TabsList>

        <TabsContent value="overview">
          <div className="grid gap-6 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Selector</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap gap-2">
                  {Object.entries(watcher.spec.selector.matchLabels || {}).map(([key, value]) => (
                    <Badge key={key} variant="default">
                      {key}={value}
                    </Badge>
                  ))}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Status</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-gray-500">Observable Pods</span>
                    <span>{watcher.status?.observablePods ?? 0}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Watched Pods</span>
                    <span>{watcher.status?.watchedPods ?? 0}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Events Detected</span>
                    <span>{watcher.status?.totalEvents ?? 0}</span>
                  </div>
                  {watcher.status?.lastEventTime && (
                    <div className="flex justify-between">
                      <span className="text-gray-500">Last Event</span>
                      <span>{new Date(watcher.status.lastEventTime).toLocaleString()}</span>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            <Card className="md:col-span-2">
              <CardHeader>
                <CardTitle>Metadata</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-4 sm:grid-cols-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-gray-500">Created</span>
                    <span>{new Date(watcher.metadata.creationTimestamp || '').toLocaleString()}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">UID</span>
                    <span className="font-mono text-xs">{watcher.metadata.uid}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Generation</span>
                    <span>{watcher.metadata.generation}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Resource Version</span>
                    <span>{watcher.metadata.resourceVersion}</span>
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="subjects">
          <div className="space-y-4">
            {watcher.spec.subjects.map((subject, index) => (
              <Card key={index}>
                <CardHeader>
                  <CardTitle className="text-base">Subject {index + 1}</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-4">
                    <div>
                      <p className="text-sm font-medium mb-2">Paths</p>
                      <div className="space-y-1">
                        {subject.paths.map((path, i) => (
                          <div key={i} className="flex items-center gap-2 font-mono text-sm bg-gray-100 dark:bg-gray-800 px-3 py-1 rounded">
                            <FileText className="h-4 w-4 text-gray-400" />
                            {path}
                          </div>
                        ))}
                      </div>
                    </div>
                    <div>
                      <p className="text-sm font-medium mb-2">Events</p>
                      <div className="flex flex-wrap gap-2">
                        {subject.events.map((event) => (
                          <Badge key={event} variant="default">{event}</Badge>
                        ))}
                      </div>
                    </div>
                    {subject.recursive !== undefined && (
                      <div className="text-sm">
                        <span className="text-gray-500">Recursive: </span>
                        <span>{subject.recursive ? 'Yes' : 'No'}</span>
                      </div>
                    )}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="yaml">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle>YAML Manifest</CardTitle>
              <Button variant="ghost" size="sm" onClick={copyYaml}>
                <Copy className="h-4 w-4 mr-2" />
                Copy
              </Button>
            </CardHeader>
            <CardContent>
              <pre className="overflow-auto bg-gray-100 dark:bg-gray-900 p-4 rounded-lg text-sm font-mono">
                {YAML.stringify(watcher)}
              </pre>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <ConfirmDialog
        open={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDelete}
        title="Delete Watcher"
        description={`Are you sure you want to delete "${watcher.metadata.name}"? This action cannot be undone.`}
        confirmText="Delete"
        variant="destructive"
        loading={deleteWatcher.isPending}
      />
    </div>
  );
}
