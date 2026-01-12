import { Suspense } from 'react';
import Link from 'next/link';
import { Plus } from 'lucide-react';
import { listArgusWatchers } from '@/lib/k8s';
import { Button } from '@/components/ui/button';
import { SkeletonTable } from '@/components/ui/skeleton';
import { Card } from '@/components/ui/card';
import { WatcherTable } from '@/components/watchers/WatcherTable';

export const dynamic = 'force-dynamic';

async function WatcherTableLoader() {
  const watchers = await listArgusWatchers();
  return <WatcherTable initialData={watchers} />;
}

export default function WatchersPage() {
  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">ArgusWatchers</h1>
          <p className="text-gray-500 dark:text-gray-400">
            File Integrity Monitoring - Watch for file changes in your pods
          </p>
        </div>
        <Link href="/watchers/new">
          <Button>
            <Plus className="h-4 w-4 mr-2" />
            New Watcher
          </Button>
        </Link>
      </div>

      <Suspense fallback={
        <Card className="p-4">
          <SkeletonTable rows={5} />
        </Card>
      }>
        <WatcherTableLoader />
      </Suspense>
    </div>
  );
}
