import { NextRequest, NextResponse } from 'next/server';
import * as k8s from '@kubernetes/client-node';
import * as stream from 'stream';

// Initialize Kubernetes client
const kc = new k8s.KubeConfig();
let k8sConfigured = false;

try {
  kc.loadFromDefault();
  k8sConfigured = true;
} catch {
  try {
    kc.loadFromCluster();
    k8sConfigured = true;
  } catch {
    console.warn('K8s: No configuration found for filesystem API');
  }
}

const exec = k8sConfigured ? new k8s.Exec(kc) : null;

interface FileEntry {
  name: string;
  path: string;
  type: 'file' | 'directory' | 'symlink';
  size: number;
  mode: string;
  modTime: string;
}

/**
 * Execute a command in a container and return stdout
 */
async function execInContainer(
  namespace: string,
  pod: string,
  container: string,
  command: string[]
): Promise<string> {
  if (!exec) {
    throw new Error('Kubernetes not configured');
  }

  return new Promise((resolve, reject) => {
    let stdout = '';
    let stderr = '';

    const stdoutStream = new stream.Writable({
      write(chunk, _encoding, callback) {
        stdout += chunk.toString();
        callback();
      },
    });

    const stderrStream = new stream.Writable({
      write(chunk, _encoding, callback) {
        stderr += chunk.toString();
        callback();
      },
    });

    exec.exec(
      namespace,
      pod,
      container,
      command,
      stdoutStream,
      stderrStream,
      null, // stdin
      false, // tty
      (status: k8s.V1Status) => {
        if (status.status === 'Success') {
          resolve(stdout);
        } else {
          // Some commands return non-zero but still produce useful output
          if (stdout) {
            resolve(stdout);
          } else {
            reject(new Error(stderr || status.message || 'Command failed'));
          }
        }
      }
    ).catch(reject);
  });
}

/**
 * Parse ls -la output into FileEntry objects
 * Format: drwxr-xr-x   2 root root  4096 Jan 10 12:00 dirname
 */
function parseLsOutput(output: string, basePath: string): FileEntry[] {
  const entries: FileEntry[] = [];
  const lines = output.trim().split('\n');

  for (const line of lines) {
    // Skip empty lines and total line
    if (!line || line.startsWith('total ')) continue;

    // Parse ls -la output
    // Format: permissions links owner group size month day time name
    const match = line.match(/^([drwxlst-]{10})\s+\d+\s+\S+\s+\S+\s+(\d+)\s+(\S+\s+\d+\s+[\d:]+)\s+(.+)$/);
    if (!match) continue;

    const [, permissions, size, dateStr, name] = match;

    // Skip . and ..
    if (name === '.' || name === '..') continue;

    // Handle symlinks (name -> target)
    let actualName = name;
    if (name.includes(' -> ')) {
      actualName = name.split(' -> ')[0];
    }

    // Determine type from permissions
    let type: 'file' | 'directory' | 'symlink' = 'file';
    if (permissions.startsWith('d')) {
      type = 'directory';
    } else if (permissions.startsWith('l')) {
      type = 'symlink';
    }

    // Build full path
    const fullPath = basePath === '/' ? `/${actualName}` : `${basePath}/${actualName}`;

    entries.push({
      name: actualName,
      path: fullPath,
      type,
      size: parseInt(size, 10),
      mode: permissions,
      modTime: new Date(dateStr).toISOString(),
    });
  }

  // Sort: directories first, then alphabetically
  entries.sort((a, b) => {
    if (a.type === 'directory' && b.type !== 'directory') return -1;
    if (a.type !== 'directory' && b.type === 'directory') return 1;
    return a.name.localeCompare(b.name);
  });

  return entries;
}

export async function GET(request: NextRequest) {
  const namespace = request.nextUrl.searchParams.get('namespace');
  const pod = request.nextUrl.searchParams.get('pod');
  const container = request.nextUrl.searchParams.get('container');
  const path = request.nextUrl.searchParams.get('path') || '/';

  if (!namespace || !pod) {
    return NextResponse.json(
      { error: 'Namespace and pod are required' },
      { status: 400 }
    );
  }

  // If Kubernetes not configured, return mock data for development
  if (!k8sConfigured || !exec) {
    console.log('FS API: Using mock data (K8s not configured)');
    return NextResponse.json({ data: getMockFilesystem(path) });
  }

  try {
    // Use ls -la to get detailed file listing
    // If container not specified, K8s will use the first container
    const containerName = container || '';

    // Sanitize path to prevent command injection
    const sanitizedPath = path.replace(/[;&|`$(){}[\]<>\\'"!]/g, '');

    const output = await execInContainer(
      namespace,
      pod,
      containerName,
      ['ls', '-la', sanitizedPath]
    );

    const entries = parseLsOutput(output, sanitizedPath);
    return NextResponse.json({ data: entries });
  } catch (error) {
    console.error('FS API error:', error);

    // If exec fails (container doesn't have ls, permission denied, etc.)
    // return a helpful error
    const message = error instanceof Error ? error.message : 'Failed to list directory';

    // Check for common errors
    if (message.includes('not found') || message.includes('No such file')) {
      return NextResponse.json(
        { error: 'Path not found', data: [] },
        { status: 404 }
      );
    }

    if (message.includes('Permission denied')) {
      return NextResponse.json(
        { error: 'Permission denied', data: [] },
        { status: 403 }
      );
    }

    // For other errors, return mock data as fallback with a warning
    console.warn('FS API: Falling back to mock data due to error:', message);
    return NextResponse.json({
      data: getMockFilesystem(path),
      warning: 'Using mock data - could not execute in container',
    });
  }
}

/**
 * Mock filesystem for development/fallback
 */
function getMockFilesystem(path: string): FileEntry[] {
  const mockFilesystem: Record<string, Array<{ name: string; type: 'file' | 'directory' | 'symlink'; size?: number }>> = {
    '/': [
      { name: 'bin', type: 'directory' },
      { name: 'etc', type: 'directory' },
      { name: 'home', type: 'directory' },
      { name: 'tmp', type: 'directory' },
      { name: 'usr', type: 'directory' },
      { name: 'var', type: 'directory' },
      { name: 'app', type: 'directory' },
      { name: 'proc', type: 'directory' },
    ],
    '/etc': [
      { name: 'passwd', type: 'file', size: 2458 },
      { name: 'shadow', type: 'file', size: 1024 },
      { name: 'hosts', type: 'file', size: 256 },
      { name: 'resolv.conf', type: 'file', size: 128 },
      { name: 'nginx', type: 'directory' },
      { name: 'ssl', type: 'directory' },
    ],
    '/etc/nginx': [
      { name: 'nginx.conf', type: 'file', size: 4096 },
      { name: 'mime.types', type: 'file', size: 2048 },
      { name: 'conf.d', type: 'directory' },
    ],
    '/etc/nginx/conf.d': [
      { name: 'default.conf', type: 'file', size: 1024 },
    ],
    '/var': [
      { name: 'log', type: 'directory' },
      { name: 'lib', type: 'directory' },
      { name: 'run', type: 'directory' },
      { name: 'cache', type: 'directory' },
    ],
    '/var/log': [
      { name: 'syslog', type: 'file', size: 524288 },
      { name: 'auth.log', type: 'file', size: 65536 },
      { name: 'nginx', type: 'directory' },
    ],
    '/var/log/nginx': [
      { name: 'access.log', type: 'file', size: 1048576 },
      { name: 'error.log', type: 'file', size: 32768 },
    ],
    '/app': [
      { name: 'config.json', type: 'file', size: 512 },
      { name: 'data', type: 'directory' },
      { name: 'src', type: 'directory' },
    ],
    '/app/data': [
      { name: 'database.db', type: 'file', size: 2097152 },
      { name: 'cache', type: 'directory' },
    ],
    '/home': [
      { name: 'app', type: 'directory' },
    ],
    '/home/app': [
      { name: '.bashrc', type: 'file', size: 256 },
      { name: '.profile', type: 'file', size: 128 },
    ],
    '/tmp': [
      { name: 'session-123', type: 'file', size: 64 },
      { name: 'cache-data', type: 'directory' },
    ],
  };

  const entries = mockFilesystem[path] || [];
  return entries.map((entry) => ({
    name: entry.name,
    path: path === '/' ? `/${entry.name}` : `${path}/${entry.name}`,
    type: entry.type,
    size: entry.size || (entry.type === 'directory' ? 4096 : 0),
    mode: entry.type === 'directory' ? 'drwxr-xr-x' : '-rw-r--r--',
    modTime: new Date().toISOString(),
  }));
}
