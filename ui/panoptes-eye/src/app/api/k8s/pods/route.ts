import { NextRequest, NextResponse } from 'next/server';
import { listPods, K8sError } from '@/lib/k8s';

export async function GET(request: NextRequest) {
  try {
    const labelSelector = request.nextUrl.searchParams.get('labelSelector') || undefined;
    const namespace = request.nextUrl.searchParams.get('namespace') || undefined;

    const pods = await listPods(labelSelector, namespace);
    return NextResponse.json({ data: pods });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to list pods' },
      { status: 500 }
    );
  }
}
