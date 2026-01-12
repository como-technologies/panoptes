import { NextResponse } from 'next/server';
import { getDaemonMetrics } from '@/lib/k8s';

export async function GET() {
  try {
    const metrics = await getDaemonMetrics();
    return NextResponse.json({ data: metrics });
  } catch (error) {
    console.error('Failed to get daemon metrics:', error);
    return NextResponse.json(
      { error: 'Failed to get daemon metrics', details: error instanceof Error ? error.message : String(error) },
      { status: 500 }
    );
  }
}
