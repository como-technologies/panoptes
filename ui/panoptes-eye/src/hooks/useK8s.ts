'use client';

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import type { ArgusWatcher, ArgusWatcherInput } from '@/types/argus';
import type { JanusGuard, JanusGuardInput } from '@/types/janus';
import type { Pod, DashboardStats, DaemonMetrics } from '@/types';

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: 'Request failed' }));
    throw new Error(error.error || 'Request failed');
  }
  const json = await res.json();
  return json.data;
}

async function mutateJson<T>(
  url: string,
  method: 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  body?: unknown
): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: 'Request failed' }));
    throw new Error(error.error || 'Request failed');
  }
  const json = await res.json();
  return json.data;
}

// Dashboard Stats
export function useDashboardStats() {
  return useQuery({
    queryKey: ['dashboardStats'],
    queryFn: () => fetchJson<DashboardStats>('/api/k8s/stats'),
  });
}

// Watchers
export function useWatchers(namespace?: string) {
  const params = namespace ? `?namespace=${namespace}` : '';
  return useQuery({
    queryKey: ['watchers', namespace],
    queryFn: () => fetchJson<ArgusWatcher[]>(`/api/k8s/watchers${params}`),
  });
}

export function useWatcher(name: string, namespace: string) {
  return useQuery({
    queryKey: ['watcher', name, namespace],
    queryFn: () => fetchJson<ArgusWatcher>(`/api/k8s/watchers/${name}?namespace=${namespace}`),
    enabled: !!name && !!namespace,
  });
}

export function useCreateWatcher() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: ArgusWatcherInput) =>
      mutateJson<ArgusWatcher>('/api/k8s/watchers', 'POST', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['watchers'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

export function useUpdateWatcher() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, ...input }: ArgusWatcherInput & { name: string }) =>
      mutateJson<ArgusWatcher>(`/api/k8s/watchers/${name}`, 'PUT', input),
    onSuccess: (_, { name, namespace }) => {
      queryClient.invalidateQueries({ queryKey: ['watchers'] });
      queryClient.invalidateQueries({ queryKey: ['watcher', name, namespace] });
    },
  });
}

export function useDeleteWatcher() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ name, namespace }: { name: string; namespace: string }) => {
      const res = await fetch(`/api/k8s/watchers/${name}?namespace=${namespace}`, { method: 'DELETE' });
      if (!res.ok) {
        const error = await res.json().catch(() => ({ error: 'Delete watcher failed' }));
        throw new Error(error.error || `Delete watcher failed: ${res.status} ${res.statusText}`);
      }
      return res;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['watchers'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

export function usePauseWatcher() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, namespace, paused }: { name: string; namespace: string; paused: boolean }) =>
      mutateJson<ArgusWatcher>(`/api/k8s/watchers/${name}`, 'PATCH', { namespace, paused }),
    onSuccess: (_, { name, namespace }) => {
      queryClient.invalidateQueries({ queryKey: ['watchers'] });
      queryClient.invalidateQueries({ queryKey: ['watcher', name, namespace] });
    },
  });
}

// Guards
export function useGuards(namespace?: string) {
  const params = namespace ? `?namespace=${namespace}` : '';
  return useQuery({
    queryKey: ['guards', namespace],
    queryFn: () => fetchJson<JanusGuard[]>(`/api/k8s/guards${params}`),
  });
}

export function useGuard(name: string, namespace: string) {
  return useQuery({
    queryKey: ['guard', name, namespace],
    queryFn: () => fetchJson<JanusGuard>(`/api/k8s/guards/${name}?namespace=${namespace}`),
    enabled: !!name && !!namespace,
  });
}

export function useCreateGuard() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: JanusGuardInput) =>
      mutateJson<JanusGuard>('/api/k8s/guards', 'POST', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

export function useUpdateGuard() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, ...input }: JanusGuardInput & { name: string }) =>
      mutateJson<JanusGuard>(`/api/k8s/guards/${name}`, 'PUT', input),
    onSuccess: (_, { name, namespace }) => {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['guard', name, namespace] });
    },
  });
}

export function useDeleteGuard() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ name, namespace }: { name: string; namespace: string }) => {
      const res = await fetch(`/api/k8s/guards/${name}?namespace=${namespace}`, { method: 'DELETE' });
      if (!res.ok) {
        const error = await res.json().catch(() => ({ error: 'Delete guard failed' }));
        throw new Error(error.error || `Delete guard failed: ${res.status} ${res.statusText}`);
      }
      return res;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

export function usePauseGuard() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, namespace, paused }: { name: string; namespace: string; paused: boolean }) =>
      mutateJson<JanusGuard>(`/api/k8s/guards/${name}`, 'PATCH', { namespace, paused }),
    onSuccess: (_, { name, namespace }) => {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['guard', name, namespace] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

export function useSetGuardEnforcing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, namespace, enforcing }: { name: string; namespace: string; enforcing: boolean }) =>
      mutateJson<JanusGuard>(`/api/k8s/guards/${name}`, 'PATCH', { namespace, enforcing }),
    onSuccess: (_, { name, namespace }) => {
      queryClient.invalidateQueries({ queryKey: ['guards'] });
      queryClient.invalidateQueries({ queryKey: ['guard', name, namespace] });
      queryClient.invalidateQueries({ queryKey: ['dashboardStats'] });
    },
  });
}

// Metrics
export function useDaemonMetrics() {
  return useQuery({
    queryKey: ['daemonMetrics'],
    queryFn: () => fetchJson<DaemonMetrics>('/api/k8s/metrics'),
    refetchInterval: 30000, // Refresh every 30 seconds
  });
}

// Pods
export function usePods(labelSelector?: string, namespace?: string) {
  const params = new URLSearchParams();
  if (labelSelector) params.set('labelSelector', labelSelector);
  if (namespace) params.set('namespace', namespace);
  const query = params.toString() ? `?${params.toString()}` : '';

  return useQuery({
    queryKey: ['pods', labelSelector, namespace],
    queryFn: () => fetchJson<Pod[]>(`/api/k8s/pods${query}`),
    enabled: !!labelSelector || !!namespace,
  });
}
