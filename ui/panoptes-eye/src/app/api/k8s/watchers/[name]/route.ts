import { NextRequest, NextResponse } from 'next/server';
import {
  getArgusWatcher,
  updateArgusWatcher,
  deleteArgusWatcher,
  pauseArgusWatcher,
  K8sError
} from '@/lib/k8s';
import type { ArgusWatcherInput } from '@/types/argus';

interface RouteParams {
  params: Promise<{ name: string }>;
}

export async function GET(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const namespace = request.nextUrl.searchParams.get('namespace');
    if (!namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    const watcher = await getArgusWatcher(name, namespace);
    return NextResponse.json({ data: watcher });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to get watcher' },
      { status: 500 }
    );
  }
}

export async function PUT(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const body = await request.json() as ArgusWatcherInput;

    if (!body.namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    const watcher = await updateArgusWatcher({
      ...body,
      name,
    });
    return NextResponse.json({ data: watcher });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to update watcher' },
      { status: 500 }
    );
  }
}

export async function DELETE(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const namespace = request.nextUrl.searchParams.get('namespace');
    if (!namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    await deleteArgusWatcher(name, namespace);
    return NextResponse.json({ success: true });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to delete watcher' },
      { status: 500 }
    );
  }
}

export async function PATCH(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const body = await request.json() as { namespace: string; paused?: boolean };

    if (!body.namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    if (typeof body.paused === 'boolean') {
      const watcher = await pauseArgusWatcher(name, body.namespace, body.paused);
      return NextResponse.json({ data: watcher });
    }

    return NextResponse.json(
      { error: 'No valid patch operation specified' },
      { status: 400 }
    );
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to patch watcher' },
      { status: 500 }
    );
  }
}
