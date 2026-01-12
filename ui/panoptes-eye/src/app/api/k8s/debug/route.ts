import { NextResponse } from 'next/server';
import { getK8sDebugInfo, K8sError } from '@/lib/k8s';

export async function GET() {
  try {
    const debug = await getK8sDebugInfo();
    return NextResponse.json({ data: debug });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to get K8s debug info', details: error instanceof Error ? error.message : String(error) },
      { status: 500 }
    );
  }
}
