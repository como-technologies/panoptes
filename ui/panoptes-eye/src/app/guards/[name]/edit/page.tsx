'use client';

import { useState, useEffect, use } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Plus, Trash2, Shield } from 'lucide-react';
import { useGuard, useUpdateGuard, usePods } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Input, Textarea } from '@/components/ui/input';
import { MultiSelect } from '@/components/ui/select';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import type { JanusSubject, JanusEventType } from '@/types/janus';

const EVENT_OPTIONS = [
  { value: 'access', label: 'Access' },
  { value: 'open', label: 'Open' },
  { value: 'open_exec', label: 'Open Exec' },
  { value: 'open_write', label: 'Open Write' },
  { value: 'open_read', label: 'Open Read' },
  { value: 'close', label: 'Close' },
  { value: 'close_write', label: 'Close Write' },
  { value: 'close_nowrite', label: 'Close No Write' },
];

interface SubjectFormData {
  allow: string;
  deny: string;
  events: string[];
  audit: boolean;
}

interface PageProps {
  params: Promise<{ name: string }>;
}

export default function EditGuardPage({ params }: PageProps) {
  const { name } = use(params);
  const router = useRouter();
  const searchParams = useSearchParams();
  const namespace = searchParams.get('namespace') || 'default';
  const updateGuard = useUpdateGuard();
  const { addToast } = useToast();

  const { data: guard, isLoading, error } = useGuard(name, namespace);

  const [selectorInput, setSelectorInput] = useState('');
  const [enforcing, setEnforcing] = useState(false);
  const [subjects, setSubjects] = useState<SubjectFormData[]>([]);
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    if (guard && !initialized) {
      const labels = guard.spec.selector.matchLabels || {};
      setSelectorInput(Object.entries(labels).map(([k, v]) => `${k}=${v}`).join(', '));
      setEnforcing(guard.spec.enforcing ?? false);
      setSubjects(
        guard.spec.subjects.map((s) => ({
          allow: s.allow?.join('\n') || '',
          deny: s.deny?.join('\n') || '',
          events: s.events,
          audit: s.audit ?? true,
        }))
      );
      setInitialized(true);
    }
  }, [guard, initialized]);

  const selector = selectorInput.split(',').reduce((acc, pair) => {
    const [key, value] = pair.trim().split('=');
    if (key && value) acc[key.trim()] = value.trim();
    return acc;
  }, {} as Record<string, string>);

  const { data: matchingPods } = usePods(
    Object.entries(selector).map(([k, v]) => `${k}=${v}`).join(','),
    namespace
  );

  const addSubject = () => {
    setSubjects([
      ...subjects,
      { allow: '', deny: '', events: ['access', 'open_write'], audit: true },
    ]);
  };

  const removeSubject = (index: number) => {
    if (subjects.length > 1) {
      setSubjects(subjects.filter((_, i) => i !== index));
    }
  };

  const updateSubjectField = (index: number, field: keyof SubjectFormData, value: unknown) => {
    const updated = [...subjects];
    updated[index] = { ...updated[index], [field]: value };
    setSubjects(updated);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (Object.keys(selector).length === 0) {
      addToast({ variant: 'error', title: 'At least one selector label is required' });
      return;
    }

    const subjectsData: JanusSubject[] = subjects
      .filter((s) => (s.allow.trim() || s.deny.trim()) && s.events.length > 0)
      .map((s) => ({
        allow: s.allow ? s.allow.split('\n').map((p) => p.trim()).filter(Boolean) : undefined,
        deny: s.deny ? s.deny.split('\n').map((p) => p.trim()).filter(Boolean) : undefined,
        events: s.events as JanusEventType[],
        audit: s.audit,
      }));

    if (subjectsData.length === 0) {
      addToast({ variant: 'error', title: 'At least one subject with paths and events is required' });
      return;
    }

    try {
      await updateGuard.mutateAsync({
        name: name,
        namespace,
        selector,
        subjects: subjectsData,
        enforcing,
        paused: guard?.spec.paused,
      });
      addToast({
        variant: 'success',
        title: 'Guard updated',
      });
      router.push(`/guards/${name}?namespace=${namespace}`);
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Failed to update guard',
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    }
  };

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-64" />
        <Card>
          <CardContent className="p-6">
            <div className="space-y-4">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
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
          <p className="text-red-500">Failed to load guard</p>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <Link
          href={`/guards/${name}?namespace=${namespace}`}
          className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2"
        >
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to Guard
        </Link>
        <h1 className="text-3xl font-bold tracking-tight">Edit {name}</h1>
        <p className="text-gray-500 dark:text-gray-400">
          Update the guard configuration
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle>Enforcement Mode</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-4">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={enforcing}
                  onChange={(e) => setEnforcing(e.target.checked)}
                  className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                />
                <span className="font-medium">Enforcing Mode</span>
              </label>
              <p className="text-xs text-gray-500">
                {enforcing ? 'Denied accesses will be blocked' : 'Denied accesses will only be logged (audit mode)'}
              </p>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Pod Selector</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-1">Label Selector</label>
              <Input
                value={selectorInput}
                onChange={(e) => setSelectorInput(e.target.value)}
                placeholder="app=nginx, tier=frontend"
              />
              <p className="mt-1 text-xs text-gray-500">
                Comma-separated key=value pairs to match pods
              </p>
            </div>
            {matchingPods && matchingPods.length > 0 && (
              <div className="p-3 bg-green-50 dark:bg-green-900/20 rounded-lg">
                <p className="text-sm font-medium text-green-800 dark:text-green-400">
                  {matchingPods.length} matching pod{matchingPods.length !== 1 ? 's' : ''} found
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <CardTitle>Access Rules</CardTitle>
            <Button type="button" variant="outline" size="sm" onClick={addSubject}>
              <Plus className="h-4 w-4 mr-1" />
              Add Rule
            </Button>
          </CardHeader>
          <CardContent className="space-y-6">
            {subjects.map((subject, index) => (
              <div
                key={index}
                className="relative p-4 border border-gray-200 dark:border-gray-700 rounded-lg"
              >
                {subjects.length > 1 && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="absolute top-2 right-2"
                    onClick={() => removeSubject(index)}
                  >
                    <Trash2 className="h-4 w-4 text-red-500" />
                  </Button>
                )}

                <div className="space-y-4">
                  <div className="grid gap-4 sm:grid-cols-2">
                    <div>
                      <label className="block text-sm font-medium mb-1 text-green-600">
                        <Shield className="inline h-4 w-4 mr-1" />
                        Allow Paths (one per line)
                      </label>
                      <Textarea
                        value={subject.allow}
                        onChange={(e) => updateSubjectField(index, 'allow', e.target.value)}
                        placeholder="/var/log/*&#10;/tmp/**"
                        rows={3}
                      />
                    </div>
                    <div>
                      <label className="block text-sm font-medium mb-1 text-red-600">
                        <Shield className="inline h-4 w-4 mr-1" />
                        Deny Paths (one per line)
                      </label>
                      <Textarea
                        value={subject.deny}
                        onChange={(e) => updateSubjectField(index, 'deny', e.target.value)}
                        placeholder="/etc/passwd&#10;/etc/shadow"
                        rows={3}
                      />
                    </div>
                  </div>

                  <div>
                    <label className="block text-sm font-medium mb-1">Events to Audit</label>
                    <MultiSelect
                      options={EVENT_OPTIONS}
                      value={subject.events}
                      onChange={(value) => updateSubjectField(index, 'events', value)}
                      placeholder="Select events..."
                    />
                  </div>

                  <div className="flex items-center gap-4">
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={subject.audit}
                        onChange={(e) => updateSubjectField(index, 'audit', e.target.checked)}
                        className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                      />
                      Log all accesses (audit mode)
                    </label>
                  </div>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>

        <div className="flex items-center justify-end gap-4">
          <Link href={`/guards/${name}?namespace=${namespace}`}>
            <Button type="button" variant="ghost">
              Cancel
            </Button>
          </Link>
          <Button type="submit" loading={updateGuard.isPending}>
            Save Changes
          </Button>
        </div>
      </form>
    </div>
  );
}
