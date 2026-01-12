'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Plus, Trash2, Shield } from 'lucide-react';
import { useCreateGuard, usePods } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Input, Textarea } from '@/components/ui/input';
import { MultiSelect } from '@/components/ui/select';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
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

export default function NewGuardPage() {
  const router = useRouter();
  const createGuard = useCreateGuard();
  const { addToast } = useToast();

  const [name, setName] = useState('');
  const [namespace, setNamespace] = useState('default');
  const [selectorInput, setSelectorInput] = useState('');
  const [enforcing, setEnforcing] = useState(false);
  const [subjects, setSubjects] = useState<SubjectFormData[]>([
    { allow: '', deny: '', events: ['access', 'open_write'], audit: true },
  ]);

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

  const updateSubject = (index: number, field: keyof SubjectFormData, value: unknown) => {
    const updated = [...subjects];
    updated[index] = { ...updated[index], [field]: value };
    setSubjects(updated);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!name.trim()) {
      addToast({ variant: 'error', title: 'Name is required' });
      return;
    }

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
      await createGuard.mutateAsync({
        name: name.trim(),
        namespace,
        selector,
        subjects: subjectsData,
        enforcing,
      });
      addToast({
        variant: 'success',
        title: 'Guard created',
        description: `${name} has been created successfully`,
      });
      router.push('/guards');
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Failed to create guard',
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <Link href="/guards" className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2">
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to Guards
        </Link>
        <h1 className="text-3xl font-bold tracking-tight">Create JanusGuard</h1>
        <p className="text-gray-500 dark:text-gray-400">
          Configure file access auditing and enforcement for your pods
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle>Basic Information</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <div>
                <label className="block text-sm font-medium mb-1">Name</label>
                <Input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="my-guard"
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">Namespace</label>
                <Input
                  value={namespace}
                  onChange={(e) => setNamespace(e.target.value)}
                  placeholder="default"
                  required
                />
              </div>
            </div>
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
                <div className="mt-2 flex flex-wrap gap-1">
                  {matchingPods.slice(0, 5).map((pod) => (
                    <span
                      key={pod.metadata.uid}
                      className="inline-flex items-center px-2 py-0.5 rounded text-xs bg-green-100 dark:bg-green-800 text-green-800 dark:text-green-100"
                    >
                      {pod.metadata.name}
                    </span>
                  ))}
                  {matchingPods.length > 5 && (
                    <span className="text-xs text-green-600 dark:text-green-400">
                      +{matchingPods.length - 5} more
                    </span>
                  )}
                </div>
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
                        onChange={(e) => updateSubject(index, 'allow', e.target.value)}
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
                        onChange={(e) => updateSubject(index, 'deny', e.target.value)}
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
                      onChange={(value) => updateSubject(index, 'events', value)}
                      placeholder="Select events..."
                    />
                  </div>

                  <div className="flex items-center gap-4">
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={subject.audit}
                        onChange={(e) => updateSubject(index, 'audit', e.target.checked)}
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
          <Link href="/guards">
            <Button type="button" variant="ghost">
              Cancel
            </Button>
          </Link>
          <Button type="submit" loading={createGuard.isPending}>
            Create Guard
          </Button>
        </div>
      </form>
    </div>
  );
}
