'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { Eye, Shield, FileSearch, Activity, FolderTree, Settings, ClipboardCheck } from 'lucide-react';
import { cn } from '@/lib/utils';

const navItems = [
  { href: '/', label: 'Dashboard', icon: Eye },
  { href: '/watchers', label: 'Watchers (Argus)', icon: FileSearch },
  { href: '/guards', label: 'Guards (Janus)', icon: Shield },
  { href: '/events', label: 'Events', icon: Activity },
  { href: '/compliance', label: 'Compliance', icon: ClipboardCheck },
  { href: '/explorer', label: 'File Explorer', icon: FolderTree },
  { href: '/settings', label: 'Settings', icon: Settings },
];

export function Navigation() {
  const pathname = usePathname();

  return (
    <nav className="w-64 border-r border-border bg-card p-4">
      <div className="mb-8">
        <div className="flex items-center gap-2">
          <Eye className="h-8 w-8 text-mint-dark" />
          <div>
            <h1 className="text-xl font-bold text-foreground">Panoptes Eye</h1>
            <p className="text-xs text-muted-foreground">All-seeing Security</p>
          </div>
        </div>
      </div>

      <ul className="space-y-1">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = pathname === item.href;

          return (
            <li key={item.href}>
              <Link
                href={item.href}
                className={cn(
                  'flex items-center gap-3 rounded-md px-3 py-2 transition-colors border-l-2',
                  isActive
                    ? 'border-l-mint-dark bg-mint-light text-mint-dark dark:bg-mint-dark/20 dark:text-mint'
                    : 'border-l-transparent text-muted-foreground hover:bg-secondary hover:text-foreground'
                )}
              >
                <Icon className={cn('h-5 w-5', isActive && 'text-mint-dark dark:text-mint')} />
                <span className="font-medium">{item.label}</span>
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
