'use client';

import * as React from 'react';
import { Copy, X } from 'lucide-react';
import { Dialog, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import type { StreamEvent } from '@/stores/eventStats';

interface EventDetailDialogProps {
  event: StreamEvent | null;
  onClose: () => void;
}

function DetailRow({ label, value, mono = false }: { label: string; value: React.ReactNode; mono?: boolean }) {
  if (value === undefined || value === null || value === '') return null;
  return (
    <div className="flex flex-col gap-1 py-2 border-b border-gray-100 dark:border-gray-700 last:border-0">
      <span className="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide">
        {label}
      </span>
      <span className={mono ? 'font-mono text-sm break-all' : 'text-sm'}>
        {value}
      </span>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = React.useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Button variant="ghost" size="sm" onClick={handleCopy} className="h-7 px-2">
      <Copy className="h-3 w-3 mr-1" />
      {copied ? 'Copied!' : 'Copy'}
    </Button>
  );
}

export function EventDetailDialog({ event, onClose }: EventDetailDialogProps) {
  const [activeTab, setActiveTab] = React.useState('overview');

  if (!event) return null;

  const getActionColor = (action: string) => {
    switch (action) {
      case 'denied':
        return 'error';
      case 'audit':
        return 'warning';
      default:
        return 'active';
    }
  };

  const formattedTimestamp = new Date(event.timestamp).toLocaleString();
  const rawTimestamp = event.timestamp;

  // Build JSON for raw view
  const rawJson = JSON.stringify(event, null, 2);

  return (
    <Dialog open={!!event} onClose={onClose}>
      <div className="max-h-[80vh] flex flex-col">
        <DialogHeader className="flex-shrink-0">
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2">
              Event Details
              <Badge variant={event.source === 'argus' ? 'argus' : 'janus'}>
                {event.source}
              </Badge>
              <Badge variant={getActionColor(event.action) as 'active' | 'error' | 'warning'}>
                {event.action}
              </Badge>
            </DialogTitle>
            <Button variant="ghost" size="sm" onClick={onClose} className="h-8 w-8 p-0">
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 min-h-0 flex flex-col">
          <TabsList className="flex-shrink-0">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="process">Process</TabsTrigger>
            <TabsTrigger value="metadata">Metadata</TabsTrigger>
            <TabsTrigger value="raw">Raw</TabsTrigger>
          </TabsList>

          <div className="flex-1 overflow-y-auto mt-4">
            <TabsContent value="overview" className="m-0">
              <div className="space-y-1">
                <DetailRow label="Path" value={event.path} mono />
                <DetailRow label="Event Type" value={event.eventType} />
                <DetailRow label="Action" value={event.action} />
                <DetailRow
                  label={event.source === 'argus' ? 'Watcher' : 'Guard'}
                  value={event.resourceName}
                />
                <DetailRow label="Pod" value={event.podName} />
                <DetailRow label="Namespace" value={event.namespace} />
                <DetailRow label="Node" value={event.nodeName} />
                <DetailRow label="Timestamp" value={formattedTimestamp} />
                <DetailRow label="ISO Timestamp" value={rawTimestamp} mono />
              </div>
            </TabsContent>

            <TabsContent value="process" className="m-0">
              {event.processInfo ? (
                <div className="space-y-1">
                  <DetailRow label="PID" value={event.processInfo.pid} mono />
                  <DetailRow label="PPID" value={event.processInfo.ppid} mono />
                  <DetailRow label="TID" value={event.processInfo.tid} mono />
                  <DetailRow label="UID" value={event.processInfo.uid} mono />
                  <DetailRow label="GID" value={event.processInfo.gid} mono />
                  <DetailRow label="Process Name (comm)" value={event.processInfo.comm} mono />
                  <DetailRow label="Executable (exe)" value={event.processInfo.exe} mono />
                  <DetailRow
                    label="Command Line"
                    value={event.processInfo.cmdline?.join(' ')}
                    mono
                  />
                  <DetailRow label="Working Directory (cwd)" value={event.processInfo.cwd} mono />
                </div>
              ) : (
                <div className="text-center py-8 text-gray-500 dark:text-gray-400">
                  <p className="text-sm">No process information available</p>
                  <p className="text-xs mt-1">
                    {event.source === 'argus'
                      ? 'Argus (inotify) does not capture process info yet'
                      : 'Process may have exited before info could be captured'}
                  </p>
                </div>
              )}
            </TabsContent>

            <TabsContent value="metadata" className="m-0">
              <div className="space-y-1">
                <DetailRow label="Event ID" value={event.id} mono />
                <DetailRow label="Source" value={event.source} />
                <DetailRow
                  label={event.source === 'argus' ? 'ArgusWatcher' : 'JanusGuard'}
                  value={event.resourceName}
                />
                {event.processInfo && (
                  <>
                    <DetailRow
                      label="Tags"
                      value={
                        Object.keys(event.processInfo).length > 0
                          ? Object.entries(event.processInfo)
                              .filter(([k]) => !['pid', 'tid', 'uid', 'gid', 'comm', 'exe', 'ppid', 'cmdline', 'cwd'].includes(k))
                              .map(([k, v]) => `${k}=${v}`)
                              .join(', ') || 'None'
                          : 'None'
                      }
                    />
                  </>
                )}
              </div>
            </TabsContent>

            <TabsContent value="raw" className="m-0">
              <div className="flex justify-end mb-2">
                <CopyButton text={rawJson} />
              </div>
              <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-lg text-xs font-mono overflow-x-auto whitespace-pre-wrap break-all">
                {rawJson}
              </pre>
            </TabsContent>
          </div>
        </Tabs>

        <DialogFooter className="flex-shrink-0 mt-4">
          <Button variant="ghost" onClick={onClose}>
            Close
          </Button>
        </DialogFooter>
      </div>
    </Dialog>
  );
}
