import { Suspense } from 'react';
import Link from 'next/link';
import { Plus } from 'lucide-react';
import { listJanusGuards } from '@/lib/k8s';
import { Button } from '@/components/ui/button';
import { SkeletonTable } from '@/components/ui/skeleton';
import { Card } from '@/components/ui/card';
import { GuardTable } from '@/components/guards/GuardTable';

export const dynamic = 'force-dynamic';

async function GuardTableLoader() {
  const guards = await listJanusGuards();
  return <GuardTable initialData={guards} />;
}

export default function GuardsPage() {
  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">JanusGuards</h1>
          <p className="text-gray-500 dark:text-gray-400">
            File Access Auditing - Control and audit file access in your pods
          </p>
        </div>
        <Link href="/guards/new">
          <Button>
            <Plus className="h-4 w-4 mr-2" />
            New Guard
          </Button>
        </Link>
      </div>

      <Suspense fallback={
        <Card className="p-4">
          <SkeletonTable rows={5} />
        </Card>
      }>
        <GuardTableLoader />
      </Suspense>
    </div>
  );
}
