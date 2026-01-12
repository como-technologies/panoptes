import { NextRequest, NextResponse } from 'next/server';
import { listJanusGuards, createJanusGuard, K8sError } from '@/lib/k8s';
import type { JanusGuardInput } from '@/types/janus';

export async function GET(request: NextRequest) {
  try {
    const namespace = request.nextUrl.searchParams.get('namespace') || undefined;
    const guards = await listJanusGuards(namespace);
    return NextResponse.json({ data: guards });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to list guards' },
      { status: 500 }
    );
  }
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json() as JanusGuardInput;

    if (!body.name || !body.namespace) {
      return NextResponse.json(
        { error: 'Name and namespace are required' },
        { status: 400 }
      );
    }

    const guard = await createJanusGuard(body);
    return NextResponse.json({ data: guard }, { status: 201 });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to create guard' },
      { status: 500 }
    );
  }
}
