'use client';

import { useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Edit, Trash2, RefreshCw, Copy, Shield, Play, Pause, ShieldAlert, ShieldCheck } from 'lucide-react';
import { useState, use } from 'react';
import { useGuard, useDeleteGuard, usePauseGuard, useSetGuardEnforcing } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge, ModeBadge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ConfirmDialog } from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { useRouter } from 'next/navigation';
import { getJanusMode } from '@/types/janus';
import YAML from 'yaml';

interface PageProps {
  params: Promise<{ name: string }>;
}

export default function GuardDetailPage({ params }: PageProps) {
  const { name } = use(params);
  const searchParams = useSearchParams();
  const namespace = searchParams.get('namespace') || 'default';
  const router = useRouter();
  const [showDelete, setShowDelete] = useState(false);
  const [activeTab, setActiveTab] = useState('overview');

  const { data: guard, isLoading, error, refetch } = useGuard(name, namespace);
  const deleteGuard = useDeleteGuard();
  const pauseGuard = usePauseGuard();
  const setEnforcing = useSetGuardEnforcing();
  const { addToast } = useToast();

  const handlePause = async () => {
    if (!guard) return;
    const isPaused = guard.spec.paused;
    try {
      await pauseGuard.mutateAsync({
        name: guard.metadata.name,
        namespace: guard.metadata.namespace,
        paused: !isPaused,
      });
      addToast({
        variant: 'success',
        title: isPaused ? 'Guard resumed' : 'Guard paused',
      });
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Action failed',
        description: err instanceof Error ? err.message : 'Failed to update guard',
      });
    }
  };

  const handleToggleEnforcing = async () => {
    if (!guard) return;
    const isEnforcing = guard.spec.enforcing;
    try {
      await setEnforcing.mutateAsync({
        name: guard.metadata.name,
        namespace: guard.metadata.namespace,
        enforcing: !isEnforcing,
      });
      addToast({
        variant: 'success',
        title: isEnforcing ? 'Switched to audit mode' : 'Switched to enforcing mode',
      });
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Action failed',
        description: err instanceof Error ? err.message : 'Failed to update guard',
      });
    }
  };

  const handleDelete = async () => {
    if (!guard) return;
    try {
      await deleteGuard.mutateAsync({
        name: guard.metadata.name,
        namespace: guard.metadata.namespace,
      });
      addToast({
        variant: 'success',
        title: 'Guard deleted',
      });
      router.push('/guards');
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Delete failed',
        description: err instanceof Error ? err.message : 'Failed to delete guard',
      });
    }
  };

  const copyYaml = () => {
    if (!guard) return;
    const yamlStr = YAML.stringify(guard);
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

  if (error || !guard) {
    return (
      <div className="space-y-6">
        <Link href="/guards" className="inline-flex items-center text-sm text-blue-600 hover:underline">
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to Guards
        </Link>
        <Card className="p-8 text-center">
          <p className="text-red-500 mb-4">Failed to load guard</p>
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
          <Link href="/guards" className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2">
            <ArrowLeft className="h-4 w-4 mr-1" />
            Back to Guards
          </Link>
          <h1 className="text-3xl font-bold tracking-tight">{guard.metadata.name}</h1>
          <div className="flex items-center gap-2 mt-2">
            <Badge variant="default">{guard.metadata.namespace}</Badge>
            <ModeBadge mode={getJanusMode(guard)} />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant={guard.spec.paused ? 'primary' : 'outline'}
            onClick={handlePause}
            disabled={pauseGuard.isPending}
          >
            {guard.spec.paused ? (
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
          <Button
            variant={guard.spec.enforcing ? 'outline' : 'primary'}
            onClick={handleToggleEnforcing}
            disabled={setEnforcing.isPending || guard.spec.paused}
            title={guard.spec.paused ? 'Resume guard first to change mode' : undefined}
          >
            {guard.spec.enforcing ? (
              <>
                <ShieldCheck className="h-4 w-4 mr-2" />
                Enforcing
              </>
            ) : (
              <>
                <ShieldAlert className="h-4 w-4 mr-2" />
                Audit Only
              </>
            )}
          </Button>
          <Link href={`/guards/${name}/edit?namespace=${namespace}`}>
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
                  {Object.entries(guard.spec.selector.matchLabels || {}).map(([key, value]) => (
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
                    <span>{guard.status?.observablePods ?? 0}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Guarded Pods</span>
                    <span>{guard.status?.guardedPods ?? 0}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Total Denied</span>
                    <span className="text-red-500">{guard.status?.totalDeniedEvents ?? 0}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Total Audited</span>
                    <span>{guard.status?.totalAuditEvents ?? 0}</span>
                  </div>
                  {guard.status?.lastEventTime && (
                    <div className="flex justify-between">
                      <span className="text-gray-500">Last Event</span>
                      <span>{new Date(guard.status.lastEventTime).toLocaleString()}</span>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            <Card className="md:col-span-2">
              <CardHeader>
                <CardTitle>Configuration</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-4 sm:grid-cols-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-gray-500">Mode</span>
                    <ModeBadge mode={getJanusMode(guard)} />
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Enforcing</span>
                    <span>{guard.spec.enforcing ? 'Yes' : 'No'}</span>
                  </div>
                  {guard.spec.logFormat && (
                    <div className="flex justify-between sm:col-span-2">
                      <span className="text-gray-500">Log Format</span>
                      <span className="font-mono text-xs">{guard.spec.logFormat}</span>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="subjects">
          <div className="space-y-4">
            {guard.spec.subjects.map((subject, index) => (
              <Card key={index}>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <Shield className="h-4 w-4" />
                    Subject {index + 1}
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-4">
                    {subject.allow && subject.allow.length > 0 && (
                      <div>
                        <p className="text-sm font-medium mb-2 text-green-600">Allow Paths</p>
                        <div className="space-y-1">
                          {subject.allow.map((path, i) => (
                            <div key={i} className="font-mono text-sm bg-green-50 dark:bg-green-900/20 px-3 py-1 rounded">
                              {path}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    {subject.deny && subject.deny.length > 0 && (
                      <div>
                        <p className="text-sm font-medium mb-2 text-red-600">Deny Paths</p>
                        <div className="space-y-1">
                          {subject.deny.map((path, i) => (
                            <div key={i} className="font-mono text-sm bg-red-50 dark:bg-red-900/20 px-3 py-1 rounded">
                              {path}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    <div>
                      <p className="text-sm font-medium mb-2">Events</p>
                      <div className="flex flex-wrap gap-2">
                        {subject.events.map((event) => (
                          <Badge key={event} variant="default">{event}</Badge>
                        ))}
                      </div>
                    </div>
                    {subject.audit !== undefined && (
                      <div className="text-sm">
                        <span className="text-gray-500">Audit: </span>
                        <span>{subject.audit ? 'Yes' : 'No'}</span>
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
                {YAML.stringify(guard)}
              </pre>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <ConfirmDialog
        open={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDelete}
        title="Delete Guard"
        description={`Are you sure you want to delete "${guard.metadata.name}"? This action cannot be undone.`}
        confirmText="Delete"
        variant="destructive"
        loading={deleteGuard.isPending}
      />
    </div>
  );
}
