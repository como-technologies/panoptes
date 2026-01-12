import { NextRequest, NextResponse } from 'next/server';
import { listArgusWatchers, createArgusWatcher, K8sError } from '@/lib/k8s';
import type { ArgusWatcherInput } from '@/types/argus';

export async function GET(request: NextRequest) {
  try {
    const namespace = request.nextUrl.searchParams.get('namespace') || undefined;
    const watchers = await listArgusWatchers(namespace);
    return NextResponse.json({ data: watchers });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to list watchers' },
      { status: 500 }
    );
  }
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json() as ArgusWatcherInput;

    if (!body.name || !body.namespace) {
      return NextResponse.json(
        { error: 'Name and namespace are required' },
        { status: 400 }
      );
    }

    const watcher = await createArgusWatcher(body);
    return NextResponse.json({ data: watcher }, { status: 201 });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to create watcher' },
      { status: 500 }
    );
  }
}
