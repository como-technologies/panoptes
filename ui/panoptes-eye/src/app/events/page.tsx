'use client';

import { useState } from 'react';
import { Activity, Pause, Play, Trash2, Filter, Download, AlertCircle, RotateCcw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useEventStats, useEventCounts, useRecentEvents, type StreamEvent } from '@/stores/eventStats';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { SearchInput } from '@/components/ui/input';
import { MultiSelect } from '@/components/ui/select';
import { ColumnSelector, type ColumnOption } from '@/components/ui/column-selector';
import { EventDetailDialog } from '@/components/events/EventDetailDialog';

const EVENT_TYPE_OPTIONS = [
  { value: 'access', label: 'Access' },
  { value: 'modify', label: 'Modify' },
  { value: 'create', label: 'Create' },
  { value: 'delete', label: 'Delete' },
  { value: 'open', label: 'Open' },
  { value: 'close', label: 'Close' },
];

const SOURCE_OPTIONS = [
  { value: 'argus', label: 'Argus (Watchers)' },
  { value: 'janus', label: 'Janus (Guards)' },
];

const ACTION_OPTIONS = [
  { value: 'allowed', label: 'Allowed' },
  { value: 'denied', label: 'Denied' },
  { value: 'audit', label: 'Audit' },
];

// Column definitions for the event table
const ALL_COLUMNS: ColumnOption[] = [
  { id: 'time', label: 'Time', alwaysOn: true },
  { id: 'source', label: 'Source' },
  { id: 'event', label: 'Event' },
  { id: 'path', label: 'Path', alwaysOn: true },
  { id: 'pod', label: 'Pod' },
  { id: 'action', label: 'Action' },
  { id: 'node', label: 'Node' },
  { id: 'namespace', label: 'Namespace' },
  { id: 'process', label: 'Process' },
  { id: 'pid', label: 'PID' },
];

// Default visible columns (no Process/PID by default)
const DEFAULT_VISIBLE_COLUMNS = ['time', 'source', 'event', 'path', 'pod', 'action'];

export default function EventsPage() {
  // Get events from global store (populated by EventStreamSubscriber in providers.tsx)
  const allEvents = useRecentEvents();
  const [isPaused, setIsPaused] = useState(false);
  const [search, setSearch] = useState('');
  const [eventTypes, setEventTypes] = useState<string[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [actions, setActions] = useState<string[]>([]);
  const [showFilters, setShowFilters] = useState(false);
  const [selectedEvent, setSelectedEvent] = useState<StreamEvent | null>(null);
  const [visibleColumns, setVisibleColumns] = useState<string[]>(DEFAULT_VISIBLE_COLUMNS);

  // Access event stats from Zustand store (SSR-safe)
  const eventCounts = useEventCounts();
  const { resetStats, clearEvents } = useEventStats.getState();

  // When paused, freeze the displayed events
  const [pausedEvents, setPausedEvents] = useState<StreamEvent[]>([]);
  const events = isPaused ? pausedEvents : allEvents;

  const handlePauseToggle = () => {
    if (!isPaused) {
      // Pausing: capture current events
      setPausedEvents(allEvents);
    }
    setIsPaused(!isPaused);
  };

  const filteredEvents = events.filter((event) => {
    if (search && !event.path.toLowerCase().includes(search.toLowerCase()) &&
        !event.podName.toLowerCase().includes(search.toLowerCase())) {
      return false;
    }
    if (eventTypes.length > 0 && !eventTypes.includes(event.eventType)) {
      return false;
    }
    if (sources.length > 0 && !sources.includes(event.source)) {
      return false;
    }
    if (actions.length > 0 && !actions.includes(event.action)) {
      return false;
    }
    return true;
  });

  // Helper to check if a column is visible
  const isColumnVisible = (columnId: string) => visibleColumns.includes(columnId);

  const exportEvents = () => {
    // Build headers based on visible columns
    const columnToHeader: Record<string, string> = {
      time: 'Timestamp',
      source: 'Source',
      event: 'Event Type',
      path: 'Path',
      pod: 'Pod',
      action: 'Action',
      node: 'Node',
      namespace: 'Namespace',
      process: 'Process',
      pid: 'PID',
    };

    const headers = visibleColumns.map((col) => columnToHeader[col] || col);

    const csv = [
      headers.join(','),
      ...filteredEvents.map((e) => {
        const values: string[] = [];
        for (const col of visibleColumns) {
          switch (col) {
            case 'time':
              values.push(e.timestamp);
              break;
            case 'source':
              values.push(e.source);
              break;
            case 'event':
              values.push(e.eventType);
              break;
            case 'path':
              values.push(`"${e.path}"`);
              break;
            case 'pod':
              values.push(e.podName);
              break;
            case 'action':
              values.push(e.action);
              break;
            case 'node':
              values.push(e.nodeName);
              break;
            case 'namespace':
              values.push(e.namespace || '');
              break;
            case 'process':
              values.push(e.processInfo?.comm || '');
              break;
            case 'pid':
              values.push(e.processInfo?.pid?.toString() || '');
              break;
            default:
              values.push('');
          }
        }
        return values.join(',');
      }),
    ].join('\n');

    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `events-${new Date().toISOString()}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

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

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Event Stream</h1>
          <p className="text-gray-500 dark:text-gray-400">
            Real-time file system events from Argus and Janus
          </p>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-gray-500">Session:</span>
            <Badge variant="argus">{eventCounts.argusEvents} argus</Badge>
            <Badge variant="janus">{eventCounts.janusEvents} janus</Badge>
            {eventCounts.deniedEvents > 0 && (
              <Badge variant="error">{eventCounts.deniedEvents} denied</Badge>
            )}
          </div>
          <Button variant="ghost" size="sm" onClick={resetStats} title="Reset session counts">
            <RotateCcw className="h-4 w-4" />
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <div className="flex-1 min-w-[200px] max-w-sm">
          <SearchInput
            placeholder="Search by path or pod..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onClear={() => setSearch('')}
          />
        </div>
        <Button
          variant={showFilters ? 'secondary' : 'outline'}
          size="sm"
          onClick={() => setShowFilters(!showFilters)}
        >
          <Filter className="h-4 w-4 mr-2" />
          Filters
          {(eventTypes.length > 0 || sources.length > 0 || actions.length > 0) && (
            <Badge variant="active" className="ml-2">
              {eventTypes.length + sources.length + actions.length}
            </Badge>
          )}
        </Button>
        <Button
          variant={isPaused ? 'primary' : 'outline'}
          size="sm"
          onClick={handlePauseToggle}
        >
          {isPaused ? (
            <>
              <Play className="h-4 w-4 mr-2" />
              Resume
            </>
          ) : (
            <>
              <Pause className="h-4 w-4 mr-2" />
              Pause
            </>
          )}
        </Button>
        <Button variant="outline" size="sm" onClick={clearEvents}>
          <Trash2 className="h-4 w-4 mr-2" />
          Clear
        </Button>
        <ColumnSelector
          columns={ALL_COLUMNS}
          visible={visibleColumns}
          onChange={setVisibleColumns}
        />
        <Button variant="outline" size="sm" onClick={exportEvents} disabled={events.length === 0}>
          <Download className="h-4 w-4 mr-2" />
          Export
        </Button>
      </div>

      {showFilters && (
        <Card>
          <CardContent className="p-4">
            <div className="grid gap-4 sm:grid-cols-3">
              <div>
                <label className="block text-sm font-medium mb-1">Event Types</label>
                <MultiSelect
                  options={EVENT_TYPE_OPTIONS}
                  value={eventTypes}
                  onChange={setEventTypes}
                  placeholder="All types"
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">Source</label>
                <MultiSelect
                  options={SOURCE_OPTIONS}
                  value={sources}
                  onChange={setSources}
                  placeholder="All sources"
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">Action</label>
                <MultiSelect
                  options={ACTION_OPTIONS}
                  value={actions}
                  onChange={setActions}
                  placeholder="All actions"
                />
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      <Card className="flex-1 flex flex-col min-h-0">
        <CardHeader className="pb-2 flex-shrink-0">
          <CardTitle className="text-base flex items-center gap-2">
            <Activity className="h-4 w-4" />
            Events
            <Badge variant="default">{filteredEvents.length}</Badge>
            {isPaused && <Badge variant="paused">Paused</Badge>}
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0 flex-1 min-h-0 flex flex-col">
          {events.length === 0 ? (
            <div className="p-12 text-center">
              <AlertCircle className="h-12 w-12 mx-auto text-gray-400 dark:text-gray-500 mb-4" />
              <h3 className="text-lg font-semibold">No events yet</h3>
              <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">
                Events will appear here as they are detected
              </p>
            </div>
          ) : filteredEvents.length === 0 ? (
            <div className="p-12 text-center">
              <Filter className="h-12 w-12 mx-auto text-gray-400 dark:text-gray-500 mb-4" />
              <h3 className="text-lg font-semibold">No matching events</h3>
              <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">
                Try adjusting your filters
              </p>
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="bg-gray-50 dark:bg-gray-800/50 sticky top-0">
                  <tr>
                    {isColumnVisible('time') && (
                      <th className="px-4 py-2 text-left font-medium">Time</th>
                    )}
                    {isColumnVisible('source') && (
                      <th className="px-4 py-2 text-left font-medium">Source</th>
                    )}
                    {isColumnVisible('event') && (
                      <th className="px-4 py-2 text-left font-medium">Event</th>
                    )}
                    {isColumnVisible('path') && (
                      <th className="px-4 py-2 text-left font-medium">Path</th>
                    )}
                    {isColumnVisible('pod') && (
                      <th className="px-4 py-2 text-left font-medium">Pod</th>
                    )}
                    {isColumnVisible('action') && (
                      <th className="px-4 py-2 text-left font-medium">Action</th>
                    )}
                    {isColumnVisible('node') && (
                      <th className="px-4 py-2 text-left font-medium">Node</th>
                    )}
                    {isColumnVisible('namespace') && (
                      <th className="px-4 py-2 text-left font-medium">Namespace</th>
                    )}
                    {isColumnVisible('process') && (
                      <th className="px-4 py-2 text-left font-medium">Process</th>
                    )}
                    {isColumnVisible('pid') && (
                      <th className="px-4 py-2 text-left font-medium">PID</th>
                    )}
                  </tr>
                </thead>
                <tbody className="divide-y dark:divide-gray-700">
                  {filteredEvents.map((event) => (
                    <tr
                      key={event.id}
                      className="hover:bg-gray-50 dark:hover:bg-gray-800/50 cursor-pointer"
                      onClick={() => setSelectedEvent(event)}
                    >
                      {isColumnVisible('time') && (
                        <td className="px-4 py-2 whitespace-nowrap text-gray-500 dark:text-gray-400 font-mono text-xs">
                          {new Date(event.timestamp).toLocaleTimeString()}
                        </td>
                      )}
                      {isColumnVisible('source') && (
                        <td className="px-4 py-2">
                          <Badge variant={event.source === 'argus' ? 'argus' : 'janus'}>
                            {event.source}
                          </Badge>
                        </td>
                      )}
                      {isColumnVisible('event') && (
                        <td className="px-4 py-2 capitalize">{event.eventType}</td>
                      )}
                      {isColumnVisible('path') && (
                        <td className="px-4 py-2 font-mono text-xs max-w-xs truncate" title={event.path}>
                          {event.path}
                        </td>
                      )}
                      {isColumnVisible('pod') && (
                        <td className="px-4 py-2 text-gray-500 dark:text-gray-400">
                          {event.podName}
                        </td>
                      )}
                      {isColumnVisible('action') && (
                        <td className="px-4 py-2">
                          <Badge variant={getActionColor(event.action) as 'active' | 'error' | 'warning'}>
                            {event.action}
                          </Badge>
                        </td>
                      )}
                      {isColumnVisible('node') && (
                        <td className="px-4 py-2 text-gray-500 dark:text-gray-400">
                          {event.nodeName}
                        </td>
                      )}
                      {isColumnVisible('namespace') && (
                        <td className="px-4 py-2 text-gray-500 dark:text-gray-400">
                          {event.namespace || '-'}
                        </td>
                      )}
                      {isColumnVisible('process') && (
                        <td className="px-4 py-2 font-mono text-xs" title={event.processInfo?.exe || ''}>
                          {event.processInfo?.comm || '-'}
                        </td>
                      )}
                      {isColumnVisible('pid') && (
                        <td className="px-4 py-2 font-mono text-xs">
                          {event.processInfo?.pid || '-'}
                        </td>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Event Detail Dialog */}
      <EventDetailDialog
        event={selectedEvent}
        onClose={() => setSelectedEvent(null)}
      />
    </div>
  );
}
