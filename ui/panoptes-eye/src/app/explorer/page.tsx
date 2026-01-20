'use client';

import { useState, useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Folder, File, FileSymlink, ChevronRight, ChevronDown, Search, RefreshCw, Home, Eye, Shield } from 'lucide-react';
import Link from 'next/link';
import { usePods } from '@/hooks/useK8s';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Input, SearchInput } from '@/components/ui/input';
import { Select } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import type { FileEntry, Pod, Container } from '@/types/k8s';

interface TreeNodeProps {
  entry: FileEntry;
  level: number;
  expanded: Set<string>;
  onToggle: (path: string) => void;
  onSelect: (entry: FileEntry) => void;
  selected: string | null;
  namespace: string;
  pod: string;
  container: string;
}

function TreeNode({ entry, level, expanded, onToggle, onSelect, selected, namespace, pod, container }: TreeNodeProps) {
  const isExpanded = expanded.has(entry.path);
  const isSelected = selected === entry.path;
  const isDirectory = entry.type === 'directory';

  const { data: children, isLoading } = useQuery({
    queryKey: ['fs', namespace, pod, container, entry.path],
    queryFn: async () => {
      const params = new URLSearchParams({
        namespace,
        pod,
        container,
        path: entry.path,
      });
      const res = await fetch(`/api/fs?${params}`);
      if (!res.ok) throw new Error('Failed to fetch');
      const json = await res.json();
      return json.data as FileEntry[];
    },
    enabled: isDirectory && isExpanded,
  });

  const Icon = entry.type === 'directory' ? Folder : entry.type === 'symlink' ? FileSymlink : File;

  return (
    <div>
      <div
        className={`flex items-center gap-1 px-2 py-1 cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 rounded ${
          isSelected ? 'bg-blue-50 dark:bg-blue-900/30' : ''
        }`}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={() => {
          if (isDirectory) {
            onToggle(entry.path);
          }
          onSelect(entry);
        }}
      >
        {isDirectory && (
          <span className="text-gray-400">
            {isExpanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
          </span>
        )}
        {!isDirectory && <span className="w-4" />}
        <Icon className={`h-4 w-4 ${
          entry.type === 'directory' ? 'text-yellow-500' :
          entry.type === 'symlink' ? 'text-purple-500' : 'text-gray-400'
        }`} />
        <span className="text-sm truncate">{entry.name}</span>
      </div>
      {isDirectory && isExpanded && (
        <div>
          {isLoading ? (
            <div className="py-2" style={{ paddingLeft: `${(level + 1) * 16 + 8}px` }}>
              <Skeleton className="h-4 w-32" />
            </div>
          ) : (
            children?.map((child) => (
              <TreeNode
                key={child.path}
                entry={child}
                level={level + 1}
                expanded={expanded}
                onToggle={onToggle}
                onSelect={onSelect}
                selected={selected}
                namespace={namespace}
                pod={pod}
                container={container}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

export default function ExplorerPage() {
  const [namespace, setNamespace] = useState('default');
  const [labelSelector, setLabelSelector] = useState('');
  const [selectedPod, setSelectedPod] = useState<string>('');
  const [selectedContainer, setSelectedContainer] = useState<string>('');
  const [expanded, setExpanded] = useState<Set<string>>(new Set(['/']));
  const [selectedEntry, setSelectedEntry] = useState<FileEntry | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const { data: pods, isLoading: podsLoading, refetch: refetchPods } = usePods(labelSelector, namespace);

  const currentPod = pods?.find((p) => p.metadata.name === selectedPod);
  const containers: Container[] = currentPod ? [...(currentPod.spec.containers || []), ...(currentPod.spec.initContainers || [])] : [];

  const { data: rootEntries, isLoading: rootLoading } = useQuery({
    queryKey: ['fs', namespace, selectedPod, selectedContainer, '/'],
    queryFn: async () => {
      const params = new URLSearchParams({
        namespace,
        pod: selectedPod,
        container: selectedContainer,
        path: '/',
      });
      const res = await fetch(`/api/fs?${params}`);
      if (!res.ok) throw new Error('Failed to fetch');
      const json = await res.json();
      return json.data as FileEntry[];
    },
    enabled: !!selectedPod && !!selectedContainer,
  });

  const handleToggle = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const handleSelect = useCallback((entry: FileEntry) => {
    setSelectedEntry(entry);
    setSelectedPath(entry.path);
  }, []);

  const addToResource = (type: 'watcher' | 'guard') => {
    if (selectedEntry) {
      // Navigate to resource creation with pre-filled path
      const params = new URLSearchParams({
        path: selectedEntry.path,
        namespace,
        selector: labelSelector,
        type,
      });
      window.location.href = `/watchers/new?${params}`;
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">File Explorer</h1>
        <p className="text-gray-500 dark:text-gray-400">
          Browse container filesystems and select paths to watch
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Select Pod</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-3">
            <div>
              <label className="block text-sm font-medium mb-1">Namespace</label>
              <Input
                value={namespace}
                onChange={(e) => setNamespace(e.target.value)}
                placeholder="default"
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Label Selector</label>
              <div className="flex gap-2">
                <Input
                  value={labelSelector}
                  onChange={(e) => setLabelSelector(e.target.value)}
                  placeholder="app=nginx"
                />
                <Button variant="outline" onClick={() => refetchPods()}>
                  <Search className="h-4 w-4" />
                </Button>
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Pod</label>
              <Select
                options={(pods || []).map((p) => ({ value: p.metadata.name, label: p.metadata.name }))}
                value={selectedPod}
                onChange={(e) => {
                  setSelectedPod(e.target.value);
                  setSelectedContainer('');
                  setExpanded(new Set(['/']));
                  setSelectedEntry(null);
                }}
                placeholder="Select a pod..."
              />
            </div>
          </div>
          {selectedPod && containers.length > 0 && (
            <div className="max-w-xs">
              <label className="block text-sm font-medium mb-1">Container</label>
              <Select
                options={containers.map((c) => ({ value: c.name, label: c.name }))}
                value={selectedContainer}
                onChange={(e) => {
                  setSelectedContainer(e.target.value);
                  setExpanded(new Set(['/']));
                  setSelectedEntry(null);
                }}
                placeholder="Select a container..."
              />
            </div>
          )}
        </CardContent>
      </Card>

      {selectedPod && selectedContainer && (
        <div className="grid gap-6 lg:grid-cols-3">
          <Card className="lg:col-span-2">
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-base flex items-center gap-2">
                <Folder className="h-4 w-4" />
                File Tree
              </CardTitle>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setExpanded(new Set(['/']));
                  setSelectedEntry(null);
                }}
              >
                <Home className="h-4 w-4" />
              </Button>
            </CardHeader>
            <CardContent className="p-0">
              <div className="max-h-[500px] overflow-y-auto border-t dark:border-gray-700">
                {rootLoading ? (
                  <div className="p-4 space-y-2">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-4 w-48" />
                    <Skeleton className="h-4 w-40" />
                  </div>
                ) : rootEntries?.length === 0 ? (
                  <div className="p-8 text-center text-gray-500">
                    <Folder className="h-12 w-12 mx-auto mb-4 opacity-50" />
                    <p>No files found</p>
                  </div>
                ) : (
                  rootEntries?.map((entry) => (
                    <TreeNode
                      key={entry.path}
                      entry={entry}
                      level={0}
                      expanded={expanded}
                      onToggle={handleToggle}
                      onSelect={handleSelect}
                      selected={selectedPath}
                      namespace={namespace}
                      pod={selectedPod}
                      container={selectedContainer}
                    />
                  ))
                )}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">Details</CardTitle>
            </CardHeader>
            <CardContent>
              {selectedEntry ? (
                <div className="space-y-4">
                  <div>
                    <p className="text-sm text-gray-500">Name</p>
                    <p className="font-medium">{selectedEntry.name}</p>
                  </div>
                  <div>
                    <p className="text-sm text-gray-500">Path</p>
                    <p className="font-mono text-sm break-all">{selectedEntry.path}</p>
                  </div>
                  <div>
                    <p className="text-sm text-gray-500">Type</p>
                    <Badge variant="default" className="capitalize">{selectedEntry.type}</Badge>
                  </div>
                  {selectedEntry.mode && (
                    <div>
                      <p className="text-sm text-gray-500">Permissions</p>
                      <p className="font-mono text-sm">{selectedEntry.mode}</p>
                    </div>
                  )}
                  {selectedEntry.size !== undefined && (
                    <div>
                      <p className="text-sm text-gray-500">Size</p>
                      <p>{selectedEntry.size} bytes</p>
                    </div>
                  )}
                  {selectedEntry.modTime && (
                    <div>
                      <p className="text-sm text-gray-500">Modified</p>
                      <p className="text-sm">{new Date(selectedEntry.modTime).toLocaleString()}</p>
                    </div>
                  )}
                  <div className="pt-4 border-t dark:border-gray-700 space-y-2">
                    <Button onClick={() => addToResource('watcher')} className="w-full">
                      <Eye className="h-4 w-4 mr-2" />
                      Watch Changes
                    </Button>
                    <Button onClick={() => addToResource('guard')} variant="outline" className="w-full">
                      <Shield className="h-4 w-4 mr-2" />
                      Guard Access
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="text-center text-gray-500 py-8">
                  <File className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p>Select a file or directory to view details</p>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
