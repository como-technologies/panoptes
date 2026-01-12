'use client';

import { useState } from 'react';
import Link from 'next/link';
import { Shield, Plus, Play, Pause, Trash2, Search, RefreshCw } from 'lucide-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useDeleteGuard, usePauseGuard } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { SearchInput } from '@/components/ui/input';
import { ModeBadge } from '@/components/ui/badge';
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
import type { JanusGuard } from '@/types/janus';
import { getJanusMode } from '@/types/janus';

interface GuardTableProps {
  initialData: JanusGuard[];
}

async function fetchGuards(): Promise<JanusGuard[]> {
  const res = await fetch('/api/k8s/guards');
  if (!res.ok) {
    throw new Error('Failed to fetch guards');
  }
  const json = await res.json();
  return json.data;
}

export function GuardTable({ initialData }: GuardTableProps) {
  const [search, setSearch] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<JanusGuard | null>(null);
  const queryClient = useQueryClient();

  // Use initialData for hydration, then rely on SSE for updates
  const { data: guards, isLoading, error } = useQuery({
    queryKey: ['guards'],
    queryFn: fetchGuards,
    initialData,
    staleTime: 60 * 1000,
  });

  const deleteGuard = useDeleteGuard();
  const pauseGuard = usePauseGuard();
  const { addToast } = useToast();

  const filteredGuards = (guards || []).filter((g) =>
    g.metadata.name.toLowerCase().includes(search.toLowerCase()) ||
    g.metadata.namespace.toLowerCase().includes(search.toLowerCase())
  );

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: ['guards'] });
  };

  const handlePause = async (guard: JanusGuard) => {
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
        description: `${guard.metadata.name} has been ${isPaused ? 'resumed' : 'paused'}`,
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
    if (!deleteTarget) return;
    try {
      await deleteGuard.mutateAsync({
        name: deleteTarget.metadata.name,
        namespace: deleteTarget.metadata.namespace,
      });
      addToast({
        variant: 'success',
        title: 'Guard deleted',
        description: `${deleteTarget.metadata.name} has been deleted`,
      });
      setDeleteTarget(null);
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Delete failed',
        description: err instanceof Error ? err.message : 'Failed to delete guard',
      });
    }
  };

  if (error) {
    return (
      <Card className="p-8 text-center">
        <p className="text-red-500 mb-4">Failed to load guards</p>
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
            placeholder="Search guards..."
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
      ) : filteredGuards.length === 0 && !search ? (
        <Card className="p-12 text-center">
          <Shield className="h-12 w-12 mx-auto text-gray-400 dark:text-gray-500" />
          <h3 className="mt-4 text-lg font-semibold">No guards configured</h3>
          <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
            Create a JanusGuard to start auditing file access
          </p>
          <Link href="/guards/new" className="mt-4 inline-block">
            <Button>
              <Plus className="h-4 w-4 mr-2" />
              Create Guard
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
                <TableHead>Mode</TableHead>
                <TableHead>Denied</TableHead>
                <TableHead>Audited</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredGuards.length === 0 ? (
                <TableEmptyState
                  icon={<Search className="h-8 w-8" />}
                  title="No matching guards"
                  description={`No guards match "${search}"`}
                />
              ) : (
                filteredGuards.map((guard) => (
                  <TableRow key={`${guard.metadata.namespace}/${guard.metadata.name}`}>
                    <TableCell>
                      <Link
                        href={`/guards/${guard.metadata.name}?namespace=${guard.metadata.namespace}`}
                        className="font-medium text-blue-600 hover:underline dark:text-blue-400"
                      >
                        {guard.metadata.name}
                      </Link>
                    </TableCell>
                    <TableCell className="text-gray-500 dark:text-gray-400">
                      {guard.metadata.namespace}
                    </TableCell>
                    <TableCell>
                      {guard.status?.guardedPods ?? 0} / {guard.status?.observablePods ?? 0}
                    </TableCell>
                    <TableCell>
                      <ModeBadge mode={getJanusMode(guard)} />
                    </TableCell>
                    <TableCell className="text-red-500">
                      {guard.status?.totalDeniedEvents ?? 0}
                    </TableCell>
                    <TableCell>
                      {guard.status?.totalAuditEvents ?? 0}
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handlePause(guard)}
                          title={guard.spec.paused ? 'Resume' : 'Pause'}
                          disabled={pauseGuard.isPending}
                        >
                          {guard.spec.paused ? (
                            <Play className="h-4 w-4 text-green-500" />
                          ) : (
                            <Pause className="h-4 w-4 text-amber-500" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => setDeleteTarget(guard)}
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
        title="Delete Guard"
        description={`Are you sure you want to delete "${deleteTarget?.metadata.name}"? This action cannot be undone.`}
        confirmText="Delete"
        variant="destructive"
        loading={deleteGuard.isPending}
      />
    </>
  );
}
