import { NextRequest, NextResponse } from 'next/server';
import {
  getJanusGuard,
  updateJanusGuard,
  deleteJanusGuard,
  pauseJanusGuard,
  setJanusGuardEnforcing,
  K8sError
} from '@/lib/k8s';
import type { JanusGuardInput } from '@/types/janus';

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

    const guard = await getJanusGuard(name, namespace);
    return NextResponse.json({ data: guard });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to get guard' },
      { status: 500 }
    );
  }
}

export async function PUT(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const body = await request.json() as JanusGuardInput;

    if (!body.namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    const guard = await updateJanusGuard({
      ...body,
      name,
    });
    return NextResponse.json({ data: guard });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to update guard' },
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

    await deleteJanusGuard(name, namespace);
    return NextResponse.json({ success: true });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to delete guard' },
      { status: 500 }
    );
  }
}

export async function PATCH(request: NextRequest, { params }: RouteParams) {
  try {
    const { name } = await params;
    const body = await request.json();

    if (!body.namespace) {
      return NextResponse.json(
        { error: 'Namespace is required' },
        { status: 400 }
      );
    }

    // Handle pause/resume
    if (typeof body.paused === 'boolean') {
      const guard = await pauseJanusGuard(name, body.namespace, body.paused);
      return NextResponse.json({ data: guard });
    }

    // Handle enforcing mode toggle
    if (typeof body.enforcing === 'boolean') {
      const guard = await setJanusGuardEnforcing(name, body.namespace, body.enforcing);
      return NextResponse.json({ data: guard });
    }

    return NextResponse.json(
      { error: 'No valid patch operation provided. Supported: paused, enforcing' },
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
      { error: 'Failed to patch guard' },
      { status: 500 }
    );
  }
}
