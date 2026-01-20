'use client';

import * as React from 'react';
import { Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useCreateWatcher, useCreateGuard } from '@/hooks/useK8s';
import { useToast } from '@/components/ui/toast';
import type { ArgusSubject, ArgusWatcherInput } from '@/types/argus';
import type { JanusSubject, JanusGuardInput } from '@/types/janus';
import type { RemediationResourceType } from '@/types/compliance';

export interface ResourceCreationConfig {
  resourceType: RemediationResourceType;
  name?: string;
  namespace?: string;
  selector?: Record<string, string>;
  subjects?: ArgusSubject[] | JanusSubject[];
  enforcing?: boolean;
}

interface ResourceCreationDialogProps {
  open: boolean;
  onClose: () => void;
  initialConfig?: ResourceCreationConfig;
  onSuccess?: () => void;
}

export function ResourceCreationDialog({
  open,
  onClose,
  initialConfig,
  onSuccess,
}: ResourceCreationDialogProps) {
  const { addToast } = useToast();
  const createWatcher = useCreateWatcher();
  const createGuard = useCreateGuard();

  // Form state
  const [resourceType, setResourceType] = React.useState<RemediationResourceType>('ArgusWatcher');
  const [name, setName] = React.useState('');
  const [namespace, setNamespace] = React.useState('default');
  const [selectorInput, setSelectorInput] = React.useState('');
  const [enforcing, setEnforcing] = React.useState(false);

  // Update form when initialConfig changes
  React.useEffect(() => {
    if (initialConfig) {
      setResourceType(initialConfig.resourceType);
      setName(initialConfig.name || '');
      setNamespace(initialConfig.namespace || 'default');
      if (initialConfig.selector) {
        setSelectorInput(
          Object.entries(initialConfig.selector)
            .map(([k, v]) => `${k}=${v}`)
            .join(', ')
        );
      }
      setEnforcing(initialConfig.enforcing || false);
    }
  }, [initialConfig]);

  // Reset form when dialog closes
  React.useEffect(() => {
    if (!open) {
      setName('');
      setNamespace('default');
      setSelectorInput('');
      setEnforcing(false);
    }
  }, [open]);

  const parseSelector = (input: string): Record<string, string> => {
    const result: Record<string, string> = {};
    if (!input.trim()) return result;
    input.split(',').forEach((pair) => {
      const [key, value] = pair.split('=').map((s) => s.trim());
      if (key && value) {
        result[key] = value;
      }
    });
    return result;
  };

  const formatSubjects = (): string => {
    if (!initialConfig?.subjects || initialConfig.subjects.length === 0) {
      return 'No subjects configured';
    }

    return initialConfig.subjects
      .map((subject, i) => {
        if (resourceType === 'ArgusWatcher') {
          const s = subject as ArgusSubject;
          return `${i + 1}. Paths: ${s.paths.join(', ')} | Events: ${s.events.join(', ')}`;
        } else {
          const s = subject as JanusSubject;
          const paths = s.deny?.length ? `Deny: ${s.deny.join(', ')}` : s.allow?.length ? `Allow: ${s.allow.join(', ')}` : 'No paths';
          return `${i + 1}. ${paths} | Events: ${s.events.join(', ')}`;
        }
      })
      .join('\n');
  };

  const handleSubmit = async () => {
    if (!name.trim()) {
      addToast({ title: 'Name is required', variant: 'error' });
      return;
    }

    const selector = parseSelector(selectorInput);

    try {
      if (resourceType === 'ArgusWatcher') {
        const input: ArgusWatcherInput = {
          name: name.trim(),
          namespace,
          selector,
          subjects: (initialConfig?.subjects as ArgusSubject[]) || [],
          paused: false,
        };
        await createWatcher.mutateAsync(input);
        addToast({
          title: 'ArgusWatcher created',
          description: `${name} has been created successfully`,
          variant: 'success',
        });
      } else {
        const input: JanusGuardInput = {
          name: name.trim(),
          namespace,
          selector,
          subjects: (initialConfig?.subjects as JanusSubject[]) || [],
          paused: false,
          enforcing,
        };
        await createGuard.mutateAsync(input);
        addToast({
          title: 'JanusGuard created',
          description: `${name} has been created successfully`,
          variant: 'success',
        });
      }
      onClose();
      onSuccess?.();
    } catch (error) {
      addToast({
        title: 'Failed to create resource',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'error',
      });
    }
  };

  const isLoading = createWatcher.isPending || createGuard.isPending;

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogHeader>
        <DialogTitle>
          Create {resourceType === 'ArgusWatcher' ? 'ArgusWatcher' : 'JanusGuard'}
        </DialogTitle>
        <DialogDescription>
          {resourceType === 'ArgusWatcher'
            ? 'Monitor file system changes with inotify-based detection'
            : 'Control and audit file access with fanotify-based policies'}
        </DialogDescription>
      </DialogHeader>

      <div className="space-y-4 py-4">
        {/* Resource Type Toggle */}
        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
            Resource Type
          </label>
          <div className="flex gap-2">
            <Button
              type="button"
              variant={resourceType === 'ArgusWatcher' ? 'primary' : 'ghost'}
              size="sm"
              onClick={() => setResourceType('ArgusWatcher')}
            >
              ArgusWatcher (FIM)
            </Button>
            <Button
              type="button"
              variant={resourceType === 'JanusGuard' ? 'primary' : 'ghost'}
              size="sm"
              onClick={() => setResourceType('JanusGuard')}
            >
              JanusGuard (Audit)
            </Button>
          </div>
        </div>

        {/* Name */}
        <div>
          <label htmlFor="name" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Name *
          </label>
          <Input
            id="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="my-watcher"
          />
        </div>

        {/* Namespace */}
        <div>
          <label htmlFor="namespace" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Namespace
          </label>
          <Input
            id="namespace"
            value={namespace}
            onChange={(e) => setNamespace(e.target.value)}
            placeholder="default"
          />
        </div>

        {/* Selector */}
        <div>
          <label htmlFor="selector" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Pod Selector Labels
          </label>
          <Input
            id="selector"
            value={selectorInput}
            onChange={(e) => setSelectorInput(e.target.value)}
            placeholder="app=nginx, tier=web"
          />
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Comma-separated key=value pairs to select pods
          </p>
        </div>

        {/* Enforcing (JanusGuard only) */}
        {resourceType === 'JanusGuard' && (
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="enforcing"
              checked={enforcing}
              onChange={(e) => setEnforcing(e.target.checked)}
              className="rounded border-gray-300 text-blue-600 focus:ring-blue-500"
            />
            <label htmlFor="enforcing" className="text-sm text-gray-700 dark:text-gray-300">
              Enable enforcing mode (blocks denied access)
            </label>
          </div>
        )}

        {/* Subject Preview */}
        {initialConfig?.subjects && initialConfig.subjects.length > 0 && (
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              Configured Subjects
            </label>
            <pre className="text-xs bg-gray-100 dark:bg-gray-900 p-3 rounded overflow-x-auto whitespace-pre-wrap">
              {formatSubjects()}
            </pre>
          </div>
        )}
      </div>

      <DialogFooter>
        <Button variant="ghost" onClick={onClose} disabled={isLoading}>
          Cancel
        </Button>
        <Button variant="primary" onClick={handleSubmit} loading={isLoading}>
          Create {resourceType === 'ArgusWatcher' ? 'Watcher' : 'Guard'}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
