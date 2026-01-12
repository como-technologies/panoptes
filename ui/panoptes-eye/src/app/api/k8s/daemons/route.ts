import { NextResponse } from 'next/server';
import { getDaemonHealthInfo, listPods } from '@/lib/k8s';

export async function GET() {
  try {
    // Get daemon health using label selectors
    const [argusd, janusd] = await Promise.all([
      getDaemonHealthInfo('argusd'),
      getDaemonHealthInfo('janusd'),
    ]);

    // Also get all pods in panoptes-system for debugging
    let allPodsInNamespace: Array<{ name: string; labels: Record<string, string> }> = [];
    try {
      const pods = await listPods(undefined, 'panoptes-system');
      allPodsInNamespace = pods.map(p => ({
        name: p.metadata?.name || 'unknown',
        labels: (p.metadata?.labels || {}) as Record<string, string>,
      }));
    } catch (e) {
      console.warn('Failed to list all pods in panoptes-system:', e);
    }

    return NextResponse.json({
      data: {
        argusd,
        janusd,
        k8sConfigured: !argusd.debug?.includes('coreApi is null'),
        debug: {
          allPodsInPanoptesSystem: allPodsInNamespace,
          expectedLabelSelector: 'app.kubernetes.io/name=argusd|janusd',
        },
      },
    });
  } catch (error) {
    console.error('Failed to get daemon health:', error);
    return NextResponse.json(
      { error: 'Failed to get daemon health', details: error instanceof Error ? error.message : String(error) },
      { status: 500 }
    );
  }
}
