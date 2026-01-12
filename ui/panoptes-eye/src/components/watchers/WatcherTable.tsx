'use client';

import { useState } from 'react';
import Link from 'next/link';
import { FileSearch, Plus, Play, Pause, Trash2, Search, RefreshCw } from 'lucide-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useDeleteWatcher, usePauseWatcher } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { SearchInput } from '@/components/ui/input';
import { StatusBadge } from '@/components/ui/badge';
import { ConfirmDialog } from '@/components/ui/dialog';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
  TableEmptyState,
} from '@/components/ui/table';
import { SkeletonTable } from '@/components/ui/skeleton';
import { Card } from '@/components/ui/card';
import type { ArgusWatcher } from '@/types/argus';

interface WatcherTableProps {
  initialData: ArgusWatcher[];
}

async function fetchWatchers(): Promise<ArgusWatcher[]> {
  const res = await fetch('/api/k8s/watchers');
  if (!res.ok) {
    throw new Error('Failed to fetch watchers');
  }
  const json = await res.json();
  return json.data;
}

export function WatcherTable({ initialData }: WatcherTableProps) {
  const [search, setSearch] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<ArgusWatcher | null>(null);
  const queryClient = useQueryClient();

  // Use initialData for hydration, then rely on SSE for updates
  const { data: watchers, isLoading, error } = useQuery({
    queryKey: ['watchers'],
    queryFn: fetchWatchers,
    initialData,
    staleTime: 60 * 1000,
  });

  const deleteWatcher = useDeleteWatcher();
  const pauseWatcher = usePauseWatcher();
  const { addToast } = useToast();

  const filteredWatchers = (watchers || []).filter((w) =>
    w.metadata.name.toLowerCase().includes(search.toLowerCase()) ||
    w.metadata.namespace.toLowerCase().includes(search.toLowerCase())
  );

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: ['watchers'] });
  };

  const handlePause = async (watcher: ArgusWatcher) => {
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
        description: `${watcher.metadata.name} has been ${isPaused ? 'resumed' : 'paused'}`,
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
    if (!deleteTarget) return;
    try {
      await deleteWatcher.mutateAsync({
        name: deleteTarget.metadata.name,
        namespace: deleteTarget.metadata.namespace,
      });
      addToast({
        variant: 'success',
        title: 'Watcher deleted',
        description: `${deleteTarget.metadata.name} has been deleted`,
      });
      setDeleteTarget(null);
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Delete failed',
        description: err instanceof Error ? err.message : 'Failed to delete watcher',
      });
    }
  };

  if (error) {
    return (
      <Card className="p-8 text-center">
        <p className="text-red-500 mb-4">Failed to load watchers</p>
        <Button variant="outline" onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Retry
        </Button>
      </Card>
    );
  }

  return (
    <>
      <div className="flex items-center gap-4">
        <div className="w-full max-w-sm">
          <SearchInput
            placeholder="Search watchers..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onClear={() => setSearch('')}
          />
        </div>
        <Button variant="ghost" size="sm" onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4" />
        </Button>
      </div>

      {isLoading ? (
        <Card className="p-4">
          <SkeletonTable rows={5} />
        </Card>
      ) : filteredWatchers.length === 0 && !search ? (
        <Card className="p-12 text-center">
          <FileSearch className="h-12 w-12 mx-auto text-gray-400 dark:text-gray-500" />
          <h3 className="mt-4 text-lg font-semibold">No watchers configured</h3>
          <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
            Create an ArgusWatcher to start monitoring file changes
          </p>
          <Link href="/watchers/new" className="mt-4 inline-block">
            <Button>
              <Plus className="h-4 w-4 mr-2" />
              Create Watcher
            </Button>
          </Link>
        </Card>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Namespace</TableHead>
                <TableHead>Pods</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Events</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredWatchers.length === 0 ? (
                <TableEmptyState
                  icon={<Search className="h-8 w-8" />}
                  title="No matching watchers"
                  description={`No watchers match "${search}"`}
                />
              ) : (
                filteredWatchers.map((watcher) => (
                  <TableRow key={`${watcher.metadata.namespace}/${watcher.metadata.name}`}>
                    <TableCell>
                      <Link
                        href={`/watchers/${watcher.metadata.name}?namespace=${watcher.metadata.namespace}`}
                        className="font-medium text-blue-600 hover:underline dark:text-blue-400"
                      >
                        {watcher.metadata.name}
                      </Link>
                    </TableCell>
                    <TableCell className="text-gray-500 dark:text-gray-400">
                      {watcher.metadata.namespace}
                    </TableCell>
                    <TableCell>
                      {watcher.status?.watchedPods ?? 0} / {watcher.status?.observablePods ?? 0}
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={watcher.spec.paused ? 'paused' : 'active'} />
                    </TableCell>
                    <TableCell>{watcher.status?.totalEvents ?? 0}</TableCell>
                    <TableCell>
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handlePause(watcher)}
                          disabled={pauseWatcher.isPending}
                          title={watcher.spec.paused ? 'Resume' : 'Pause'}
                        >
                          {watcher.spec.paused ? (
                            <Play className="h-4 w-4" />
                          ) : (
                            <Pause className="h-4 w-4" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => setDeleteTarget(watcher)}
                          title="Delete"
                        >
                          <Trash2 className="h-4 w-4 text-red-500" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </Card>
      )}

      <ConfirmDialog
        open={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        onConfirm={handleDelete}
        title="Delete Watcher"
        description={`Are you sure you want to delete "${deleteTarget?.metadata.name}"? This action cannot be undone.`}
        confirmText="Delete"
        variant="destructive"
        loading={deleteWatcher.isPending}
      />
    </>
  );
}
