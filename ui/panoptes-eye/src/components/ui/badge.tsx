import * as React from 'react';
import { cn } from '@/lib/utils';

type BadgeVariant = 'default' | 'active' | 'paused' | 'error' | 'enforcing' | 'audit' | 'warning' | 'argus' | 'janus';

interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

export function Badge({ className, variant = 'default', ...props }: BadgeProps) {
  const variants: Record<BadgeVariant, string> = {
    default: 'bg-gray-100 text-navy dark:bg-gray-700 dark:text-gray-300',
    active: 'bg-mint text-navy dark:bg-mint-dark/30 dark:text-mint',
    paused: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400',
    error: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400',
    enforcing: 'bg-cyan text-cyan-dark dark:bg-cyan-dark/30 dark:text-cyan',
    audit: 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400',
    warning: 'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-400',
    argus: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400',
    janus: 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400',
  };

  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium',
        variants[variant],
        className
      )}
      {...props}
    />
  );
}

interface StatusBadgeProps {
  status: 'active' | 'paused' | 'error' | 'pending';
}

export function StatusBadge({ status }: StatusBadgeProps) {
  const config: Record<string, { variant: BadgeVariant; label: string }> = {
    active: { variant: 'active', label: 'Active' },
    paused: { variant: 'paused', label: 'Paused' },
    error: { variant: 'error', label: 'Error' },
    pending: { variant: 'default', label: 'Pending' },
  };

  const { variant, label } = config[status] || config.pending;

  return (
    <Badge variant={variant} className="flex items-center gap-1">
      <span
        className={cn(
          'h-1.5 w-1.5 rounded-full',
          status === 'active' && 'bg-mint-dark',
          status === 'paused' && 'bg-yellow-500',
          status === 'error' && 'bg-red-500',
          status === 'pending' && 'bg-gray-500'
        )}
      />
      {label}
    </Badge>
  );
}

interface ModeBadgeProps {
  mode: 'enforcing' | 'audit' | 'paused';
}

export function ModeBadge({ mode }: ModeBadgeProps) {
  const variant = mode === 'paused' ? 'paused' : mode;
  const label = mode === 'enforcing' ? 'Enforcing' : mode === 'audit' ? 'Audit' : 'Paused';
  return <Badge variant={variant}>{label}</Badge>;
}
