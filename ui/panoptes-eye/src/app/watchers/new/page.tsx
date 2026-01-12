'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Plus, Trash2, FolderTree } from 'lucide-react';
import { useCreateWatcher, usePods } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Input, Textarea } from '@/components/ui/input';
import { MultiSelect } from '@/components/ui/select';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import type { ArgusSubject, ArgusEventType } from '@/types/argus';

const EVENT_OPTIONS = [
  { value: 'access', label: 'Access' },
  { value: 'attrib', label: 'Attrib' },
  { value: 'close_write', label: 'Close Write' },
  { value: 'close_nowrite', label: 'Close No Write' },
  { value: 'create', label: 'Create' },
  { value: 'delete', label: 'Delete' },
  { value: 'modify', label: 'Modify' },
  { value: 'moved_from', label: 'Moved From' },
  { value: 'moved_to', label: 'Moved To' },
  { value: 'open', label: 'Open' },
];

interface SubjectFormData {
  paths: string;
  events: string[];
  recursive: boolean;
  ignores: string;
}

export default function NewWatcherPage() {
  const router = useRouter();
  const createWatcher = useCreateWatcher();
  const { addToast } = useToast();

  const [name, setName] = useState('');
  const [namespace, setNamespace] = useState('default');
  const [selectorInput, setSelectorInput] = useState('');
  const [subjects, setSubjects] = useState<SubjectFormData[]>([
    { paths: '', events: ['modify', 'create', 'delete'], recursive: true, ignores: '' },
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
      { paths: '', events: ['modify', 'create', 'delete'], recursive: true, ignores: '' },
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

    const subjectsData: ArgusSubject[] = subjects
      .filter((s) => s.paths.trim() && s.events.length > 0)
      .map((s) => ({
        paths: s.paths.split('\n').map((p) => p.trim()).filter(Boolean),
        events: s.events as ArgusEventType[],
        recursive: s.recursive,
        ignores: s.ignores ? s.ignores.split('\n').map((p) => p.trim()).filter(Boolean) : undefined,
      }));

    if (subjectsData.length === 0) {
      addToast({ variant: 'error', title: 'At least one subject with paths and events is required' });
      return;
    }

    try {
      await createWatcher.mutateAsync({
        name: name.trim(),
        namespace,
        selector,
        subjects: subjectsData,
      });
      addToast({
        variant: 'success',
        title: 'Watcher created',
        description: `${name} has been created successfully`,
      });
      router.push('/watchers');
    } catch (err) {
      addToast({
        variant: 'error',
        title: 'Failed to create watcher',
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <Link href="/watchers" className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2">
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to Watchers
        </Link>
        <h1 className="text-3xl font-bold tracking-tight">Create ArgusWatcher</h1>
        <p className="text-gray-500 dark:text-gray-400">
          Configure file integrity monitoring for your pods
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
                  placeholder="my-watcher"
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
            <CardTitle>Watch Subjects</CardTitle>
            <Button type="button" variant="outline" size="sm" onClick={addSubject}>
              <Plus className="h-4 w-4 mr-1" />
              Add Subject
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
                  <div>
                    <label className="block text-sm font-medium mb-1">
                      <FolderTree className="inline h-4 w-4 mr-1" />
                      Paths (one per line)
                    </label>
                    <Textarea
                      value={subject.paths}
                      onChange={(e) => updateSubject(index, 'paths', e.target.value)}
                      placeholder="/etc/passwd&#10;/etc/shadow&#10;/var/log/"
                      rows={3}
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium mb-1">Events to Watch</label>
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
                        checked={subject.recursive}
                        onChange={(e) => updateSubject(index, 'recursive', e.target.checked)}
                        className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                      />
                      Recursive (watch subdirectories)
                    </label>
                  </div>

                  <div>
                    <label className="block text-sm font-medium mb-1">Ignore Patterns (optional)</label>
                    <Textarea
                      value={subject.ignores}
                      onChange={(e) => updateSubject(index, 'ignores', e.target.value)}
                      placeholder="*.tmp&#10;*.log"
                      rows={2}
                    />
                    <p className="mt-1 text-xs text-gray-500">
                      Glob patterns to exclude from watching
                    </p>
                  </div>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>

        <div className="flex items-center justify-end gap-4">
          <Link href="/watchers">
            <Button type="button" variant="ghost">
              Cancel
            </Button>
          </Link>
          <Button type="submit" loading={createWatcher.isPending}>
            Create Watcher
          </Button>
        </div>
      </form>
    </div>
  );
}
