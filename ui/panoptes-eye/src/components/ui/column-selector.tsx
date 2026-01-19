'use client';

import * as React from 'react';
import { Columns3 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from './button';

export interface ColumnOption {
  id: string;
  label: string;
  alwaysOn?: boolean;
}

interface ColumnSelectorProps {
  columns: ColumnOption[];
  visible: string[];
  onChange: (visible: string[]) => void;
  className?: string;
}

export function ColumnSelector({
  columns,
  visible,
  onChange,
  className,
}: ColumnSelectorProps) {
  const [isOpen, setIsOpen] = React.useState(false);
  const containerRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const toggleColumn = (columnId: string) => {
    const column = columns.find((c) => c.id === columnId);
    if (column?.alwaysOn) return;

    if (visible.includes(columnId)) {
      onChange(visible.filter((v) => v !== columnId));
    } else {
      onChange([...visible, columnId]);
    }
  };

  // Count optional columns that are visible
  const optionalVisibleCount = visible.filter(
    (v) => !columns.find((c) => c.id === v)?.alwaysOn
  ).length;

  return (
    <div ref={containerRef} className={cn('relative', className)}>
      <Button
        variant="outline"
        size="sm"
        onClick={() => setIsOpen(!isOpen)}
      >
        <Columns3 className="h-4 w-4 mr-2" />
        Columns
        {optionalVisibleCount > 0 && (
          <span className="ml-1 text-xs text-gray-500">({optionalVisibleCount})</span>
        )}
      </Button>
      {isOpen && (
        <div className="absolute right-0 z-50 mt-1 min-w-[180px] rounded-md border border-gray-200 bg-white shadow-lg dark:border-gray-700 dark:bg-gray-800">
          <div className="max-h-60 overflow-auto p-1">
            {columns.map((column) => (
              <label
                key={column.id}
                className={cn(
                  'flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 hover:bg-gray-100 dark:hover:bg-gray-700',
                  column.alwaysOn && 'opacity-50 cursor-not-allowed'
                )}
              >
                <input
                  type="checkbox"
                  checked={visible.includes(column.id)}
                  onChange={() => toggleColumn(column.id)}
                  disabled={column.alwaysOn}
                  className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                />
                <span className="text-sm">
                  {column.label}
                  {column.alwaysOn && (
                    <span className="ml-1 text-xs text-gray-400">(required)</span>
                  )}
                </span>
              </label>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
