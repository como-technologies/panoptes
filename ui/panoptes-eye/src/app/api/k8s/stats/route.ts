import { NextResponse } from 'next/server';
import { getDashboardStats, K8sError } from '@/lib/k8s';

export async function GET() {
  try {
    const stats = await getDashboardStats();
    return NextResponse.json({ data: stats });
  } catch (error) {
    if (error instanceof K8sError) {
      return NextResponse.json(
        { error: error.message },
        { status: error.statusCode || 500 }
      );
    }
    return NextResponse.json(
      { error: 'Failed to get dashboard stats' },
      { status: 500 }
    );
  }
}
