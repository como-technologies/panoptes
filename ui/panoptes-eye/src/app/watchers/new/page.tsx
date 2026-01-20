'use client';

import { useState, useEffect, Suspense } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, Plus, Trash2, FolderTree, Eye, Shield } from 'lucide-react';
import { useCreateWatcher, useCreateGuard, usePods } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import { Button } from '@/components/ui/button';
import { Input, Textarea } from '@/components/ui/input';
import { MultiSelect } from '@/components/ui/select';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import type { ArgusSubject, ArgusEventType } from '@/types/argus';
import type { JanusSubject, JanusEventType } from '@/types/janus';

// ArgusWatcher event options (inotify-based FIM)
const ARGUS_EVENT_OPTIONS = [
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

// JanusGuard event options (fanotify-based audit)
const JANUS_EVENT_OPTIONS = [
  { value: 'access', label: 'Access' },
  { value: 'open', label: 'Open' },
  { value: 'open_exec', label: 'Open Exec' },
  { value: 'open_write', label: 'Open Write' },
  { value: 'open_read', label: 'Open Read' },
  { value: 'close', label: 'Close' },
  { value: 'close_write', label: 'Close Write' },
  { value: 'close_nowrite', label: 'Close No Write' },
];

type ResourceType = 'ArgusWatcher' | 'JanusGuard';

interface ArgusSubjectFormData {
  paths: string;
  events: string[];
  recursive: boolean;
  ignores: string;
}

interface JanusSubjectFormData {
  allowPaths: string;
  denyPaths: string;
  events: string[];
  audit: boolean;
}

// Inner component that uses useSearchParams (must be in Suspense)
function NewResourceForm() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const createWatcher = useCreateWatcher();
  const createGuard = useCreateGuard();
  const { addToast } = useToast();

  // Resource type (watcher or guard)
  const [resourceType, setResourceType] = useState<ResourceType>('ArgusWatcher');
  const [name, setName] = useState('');
  const [namespace, setNamespace] = useState('default');
  const [selectorInput, setSelectorInput] = useState('');

  // ArgusWatcher subjects
  const [argusSubjects, setArgusSubjects] = useState<ArgusSubjectFormData[]>([
    { paths: '', events: ['modify', 'create', 'delete'], recursive: true, ignores: '' },
  ]);

  // JanusGuard subjects and options
  const [janusSubjects, setJanusSubjects] = useState<JanusSubjectFormData[]>([
    { allowPaths: '', denyPaths: '', events: ['access', 'open'], audit: true },
  ]);
  const [enforcing, setEnforcing] = useState(false);

  // Pre-fill form from URL query parameters
  useEffect(() => {
    const pathParam = searchParams.get('path');
    const namespaceParam = searchParams.get('namespace');
    const selectorParam = searchParams.get('selector');
    const typeParam = searchParams.get('type');

    if (namespaceParam) {
      setNamespace(namespaceParam);
    }
    if (selectorParam) {
      setSelectorInput(selectorParam);
    }
    if (typeParam === 'guard') {
      setResourceType('JanusGuard');
      if (pathParam) {
        setJanusSubjects([{
          allowPaths: '',
          denyPaths: pathParam,
          events: ['access', 'open'],
          audit: true,
        }]);
      }
    } else {
      if (pathParam) {
        setArgusSubjects([{
          paths: pathParam,
          events: ['modify', 'create', 'delete'],
          recursive: true,
          ignores: '',
        }]);
      }
    }
  }, [searchParams]);

  const selector = selectorInput.split(',').reduce((acc, pair) => {
    const [key, value] = pair.trim().split('=');
    if (key && value) acc[key.trim()] = value.trim();
    return acc;
  }, {} as Record<string, string>);

  const { data: matchingPods } = usePods(
    Object.entries(selector).map(([k, v]) => `${k}=${v}`).join(','),
    namespace
  );

  // Argus subject handlers
  const addArgusSubject = () => {
    setArgusSubjects([
      ...argusSubjects,
      { paths: '', events: ['modify', 'create', 'delete'], recursive: true, ignores: '' },
    ]);
  };

  const removeArgusSubject = (index: number) => {
    if (argusSubjects.length > 1) {
      setArgusSubjects(argusSubjects.filter((_, i) => i !== index));
    }
  };

  const updateArgusSubject = (index: number, field: keyof ArgusSubjectFormData, value: unknown) => {
    const updated = [...argusSubjects];
    updated[index] = { ...updated[index], [field]: value };
    setArgusSubjects(updated);
  };

  // Janus subject handlers
  const addJanusSubject = () => {
    setJanusSubjects([
      ...janusSubjects,
      { allowPaths: '', denyPaths: '', events: ['access', 'open'], audit: true },
    ]);
  };

  const removeJanusSubject = (index: number) => {
    if (janusSubjects.length > 1) {
      setJanusSubjects(janusSubjects.filter((_, i) => i !== index));
    }
  };

  const updateJanusSubject = (index: number, field: keyof JanusSubjectFormData, value: unknown) => {
    const updated = [...janusSubjects];
    updated[index] = { ...updated[index], [field]: value };
    setJanusSubjects(updated);
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

    try {
      if (resourceType === 'ArgusWatcher') {
        const subjectsData: ArgusSubject[] = argusSubjects
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

        await createWatcher.mutateAsync({
          name: name.trim(),
          namespace,
          selector,
          subjects: subjectsData,
        });
        addToast({
          variant: 'success',
          title: 'ArgusWatcher created',
          description: `${name} has been created successfully`,
        });
        router.push('/watchers');
      } else {
        // JanusGuard
        const subjectsData: JanusSubject[] = janusSubjects
          .filter((s) => (s.allowPaths.trim() || s.denyPaths.trim()) && s.events.length > 0)
          .map((s) => ({
            allow: s.allowPaths ? s.allowPaths.split('\n').map((p) => p.trim()).filter(Boolean) : undefined,
            deny: s.denyPaths ? s.denyPaths.split('\n').map((p) => p.trim()).filter(Boolean) : undefined,
            events: s.events as JanusEventType[],
            audit: s.audit,
          }));

        if (subjectsData.length === 0) {
          addToast({ variant: 'error', title: 'At least one subject with allow/deny paths and events is required' });
          return;
        }

        await createGuard.mutateAsync({
          name: name.trim(),
          namespace,
          selector,
          subjects: subjectsData,
          enforcing,
        });
        addToast({
          variant: 'success',
          title: 'JanusGuard created',
          description: `${name} has been created successfully`,
        });
        router.push('/guards');
      }
    } catch (err) {
      addToast({
        variant: 'error',
        title: `Failed to create ${resourceType === 'ArgusWatcher' ? 'watcher' : 'guard'}`,
        description: err instanceof Error ? err.message : 'Unknown error',
      });
    }
  };

  const isLoading = createWatcher.isPending || createGuard.isPending;

  return (
    <div className="space-y-6">
      <div>
        <Link
          href={resourceType === 'ArgusWatcher' ? '/watchers' : '/guards'}
          className="inline-flex items-center text-sm text-blue-600 hover:underline mb-2"
        >
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back to {resourceType === 'ArgusWatcher' ? 'Watchers' : 'Guards'}
        </Link>
        <h1 className="text-3xl font-bold tracking-tight">
          Create {resourceType === 'ArgusWatcher' ? 'ArgusWatcher' : 'JanusGuard'}
        </h1>
        <p className="text-gray-500 dark:text-gray-400">
          {resourceType === 'ArgusWatcher'
            ? 'Configure file integrity monitoring for your pods'
            : 'Configure file access auditing and control for your pods'}
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-6">
        {/* Resource Type Selection */}
        <Card>
          <CardHeader>
            <CardTitle>Resource Type</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex gap-2">
              <Button
                type="button"
                variant={resourceType === 'ArgusWatcher' ? 'primary' : 'outline'}
                onClick={() => setResourceType('ArgusWatcher')}
              >
                <Eye className="h-4 w-4 mr-2" />
                ArgusWatcher (FIM)
              </Button>
              <Button
                type="button"
                variant={resourceType === 'JanusGuard' ? 'primary' : 'outline'}
                onClick={() => setResourceType('JanusGuard')}
              >
                <Shield className="h-4 w-4 mr-2" />
                JanusGuard (Audit)
              </Button>
            </div>
            <p className="mt-2 text-xs text-gray-500">
              {resourceType === 'ArgusWatcher'
                ? 'File Integrity Monitoring using inotify - detects file changes, creation, deletion'
                : 'File Access Auditing using fanotify - audits and controls file access attempts'}
            </p>
          </CardContent>
        </Card>

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
                  placeholder={resourceType === 'ArgusWatcher' ? 'my-watcher' : 'my-guard'}
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

        {/* ArgusWatcher Subjects */}
        {resourceType === 'ArgusWatcher' && (
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle>Watch Subjects</CardTitle>
              <Button type="button" variant="outline" size="sm" onClick={addArgusSubject}>
                <Plus className="h-4 w-4 mr-1" />
                Add Subject
              </Button>
            </CardHeader>
            <CardContent className="space-y-6">
              {argusSubjects.map((subject, index) => (
                <div
                  key={index}
                  className="relative p-4 border border-gray-200 dark:border-gray-700 rounded-lg"
                >
                  {argusSubjects.length > 1 && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="absolute top-2 right-2"
                      onClick={() => removeArgusSubject(index)}
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
                        onChange={(e) => updateArgusSubject(index, 'paths', e.target.value)}
                        placeholder="/etc/passwd&#10;/etc/shadow&#10;/var/log/"
                        rows={3}
                      />
                    </div>

                    <div>
                      <label className="block text-sm font-medium mb-1">Events to Watch</label>
                      <MultiSelect
                        options={ARGUS_EVENT_OPTIONS}
                        value={subject.events}
                        onChange={(value) => updateArgusSubject(index, 'events', value)}
                        placeholder="Select events..."
                      />
                    </div>

                    <div className="flex items-center gap-4">
                      <label className="flex items-center gap-2 text-sm">
                        <input
                          type="checkbox"
                          checked={subject.recursive}
                          onChange={(e) => updateArgusSubject(index, 'recursive', e.target.checked)}
                          className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                        />
                        Recursive (watch subdirectories)
                      </label>
                    </div>

                    <div>
                      <label className="block text-sm font-medium mb-1">Ignore Patterns (optional)</label>
                      <Textarea
                        value={subject.ignores}
                        onChange={(e) => updateArgusSubject(index, 'ignores', e.target.value)}
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
        )}

        {/* JanusGuard Subjects */}
        {resourceType === 'JanusGuard' && (
          <>
            <Card>
              <CardHeader className="flex flex-row items-center justify-between">
                <CardTitle>Access Subjects</CardTitle>
                <Button type="button" variant="outline" size="sm" onClick={addJanusSubject}>
                  <Plus className="h-4 w-4 mr-1" />
                  Add Subject
                </Button>
              </CardHeader>
              <CardContent className="space-y-6">
                {janusSubjects.map((subject, index) => (
                  <div
                    key={index}
                    className="relative p-4 border border-gray-200 dark:border-gray-700 rounded-lg"
                  >
                    {janusSubjects.length > 1 && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="absolute top-2 right-2"
                        onClick={() => removeJanusSubject(index)}
                      >
                        <Trash2 className="h-4 w-4 text-red-500" />
                      </Button>
                    )}

                    <div className="space-y-4">
                      <div>
                        <label className="block text-sm font-medium mb-1">
                          <FolderTree className="inline h-4 w-4 mr-1" />
                          Allow Paths (one per line)
                        </label>
                        <Textarea
                          value={subject.allowPaths}
                          onChange={(e) => updateJanusSubject(index, 'allowPaths', e.target.value)}
                          placeholder="/app/&#10;/var/log/"
                          rows={2}
                        />
                        <p className="mt-1 text-xs text-gray-500">
                          Paths that are allowed to be accessed
                        </p>
                      </div>

                      <div>
                        <label className="block text-sm font-medium mb-1">
                          <FolderTree className="inline h-4 w-4 mr-1" />
                          Deny Paths (one per line)
                        </label>
                        <Textarea
                          value={subject.denyPaths}
                          onChange={(e) => updateJanusSubject(index, 'denyPaths', e.target.value)}
                          placeholder="/etc/shadow&#10;/root/.ssh/"
                          rows={2}
                        />
                        <p className="mt-1 text-xs text-gray-500">
                          Paths that are denied from being accessed
                        </p>
                      </div>

                      <div>
                        <label className="block text-sm font-medium mb-1">Events to Audit</label>
                        <MultiSelect
                          options={JANUS_EVENT_OPTIONS}
                          value={subject.events}
                          onChange={(value) => updateJanusSubject(index, 'events', value)}
                          placeholder="Select events..."
                        />
                      </div>

                      <div className="flex items-center gap-4">
                        <label className="flex items-center gap-2 text-sm">
                          <input
                            type="checkbox"
                            checked={subject.audit}
                            onChange={(e) => updateJanusSubject(index, 'audit', e.target.checked)}
                            className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                          />
                          Enable audit logging
                        </label>
                      </div>
                    </div>
                  </div>
                ))}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Enforcement</CardTitle>
              </CardHeader>
              <CardContent>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={enforcing}
                    onChange={(e) => setEnforcing(e.target.checked)}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                  />
                  Enable enforcing mode (blocks denied access attempts)
                </label>
                <p className="mt-2 text-xs text-gray-500">
                  Warning: Enabling enforcement will block access to denied paths. Test in audit-only mode first.
                </p>
              </CardContent>
            </Card>
          </>
        )}

        <div className="flex items-center justify-end gap-4">
          <Link href={resourceType === 'ArgusWatcher' ? '/watchers' : '/guards'}>
            <Button type="button" variant="ghost">
              Cancel
            </Button>
          </Link>
          <Button type="submit" loading={isLoading}>
            Create {resourceType === 'ArgusWatcher' ? 'Watcher' : 'Guard'}
          </Button>
        </div>
      </form>
    </div>
  );
}

// Wrapper component with Suspense for useSearchParams
export default function NewWatcherPage() {
  return (
    <Suspense fallback={<div className="p-6">Loading...</div>}>
      <NewResourceForm />
    </Suspense>
  );
}
