import { NextResponse } from 'next/server';
import { getClusterInfo } from '@/lib/k8s';

export async function GET() {
  try {
    const info = await getClusterInfo();
    return NextResponse.json({ data: info });
  } catch (error) {
    console.error('Failed to get cluster info:', error);
    return NextResponse.json(
      { error: 'Failed to get cluster info' },
      { status: 500 }
    );
  }
}
