import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import path from 'path';
import fs from 'fs';
import type { FileEvent, AccessEvent } from './types';

// Proto file paths - try container path first, then development path
function getProtoRoot(): string {
  // In container: /app/proto
  const containerPath = '/app/proto';
  if (fs.existsSync(containerPath)) {
    console.log('gRPC: Using container proto path:', containerPath);
    return containerPath;
  }
  // Development: relative to repo root from ui/panoptes-eye
  const devPath = path.resolve(process.cwd(), '../../proto');
  if (fs.existsSync(devPath)) {
    console.log('gRPC: Using development proto path:', devPath);
    return devPath;
  }
  // Fallback
  console.warn('gRPC: Proto directory not found, trying cwd/proto');
  return path.join(process.cwd(), 'proto');
}

const PROTO_ROOT = getProtoRoot();
const ARGUS_PROTO = path.join(PROTO_ROOT, 'argus/v1/argus.proto');
const JANUS_PROTO = path.join(PROTO_ROOT, 'janus/v1/janus.proto');

// Default daemon endpoints
const DEFAULT_ARGUSD_HOST = process.env.ARGUSD_HOST || 'localhost:50051';
const DEFAULT_JANUSD_HOST = process.env.JANUSD_HOST || 'localhost:50052';

// Proto loader options
const PROTO_OPTIONS: protoLoader.Options = {
  keepCase: false,
  longs: String,
  enums: Number,
  defaults: true,
  oneofs: true,
  includeDirs: [PROTO_ROOT, path.join(PROTO_ROOT, '..')],
};

// Singleton clients
let argusdClient: grpc.Client | null = null;
let janusdClient: grpc.Client | null = null;
let argusProto: grpc.GrpcObject | null = null;
let janusProto: grpc.GrpcObject | null = null;

// Load Argus proto
async function loadArgusProto(): Promise<grpc.GrpcObject> {
  if (argusProto) return argusProto;

  try {
    const packageDefinition = await protoLoader.load(ARGUS_PROTO, PROTO_OPTIONS);
    argusProto = grpc.loadPackageDefinition(packageDefinition);
    return argusProto;
  } catch (error) {
    console.error('Failed to load Argus proto:', error);
    throw error;
  }
}

// Load Janus proto
async function loadJanusProto(): Promise<grpc.GrpcObject> {
  if (janusProto) return janusProto;

  try {
    const packageDefinition = await protoLoader.load(JANUS_PROTO, PROTO_OPTIONS);
    janusProto = grpc.loadPackageDefinition(packageDefinition);
    return janusProto;
  } catch (error) {
    console.error('Failed to load Janus proto:', error);
    throw error;
  }
}

// Get Argus daemon client
export async function getArgusdClient(host?: string): Promise<grpc.Client | null> {
  const targetHost = host || DEFAULT_ARGUSD_HOST;

  try {
    const proto = await loadArgusProto();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ArgusdService = (proto as any).argus?.v1?.ArgusdService;

    if (!ArgusdService) {
      console.error('ArgusdService not found in proto');
      return null;
    }

    argusdClient = new ArgusdService(
      targetHost,
      grpc.credentials.createInsecure()
    );

    return argusdClient;
  } catch (error) {
    console.error('Failed to create Argusd client:', error);
    return null;
  }
}

// Get Janus daemon client
export async function getJanusdClient(host?: string): Promise<grpc.Client | null> {
  const targetHost = host || DEFAULT_JANUSD_HOST;

  try {
    const proto = await loadJanusProto();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const JanusdService = (proto as any).janus?.v1?.JanusdService;

    if (!JanusdService) {
      console.error('JanusdService not found in proto');
      return null;
    }

    janusdClient = new JanusdService(
      targetHost,
      grpc.credentials.createInsecure()
    );

    return janusdClient;
  } catch (error) {
    console.error('Failed to create Janusd client:', error);
    return null;
  }
}

// Event callback types
export type FileEventCallback = (event: FileEvent) => void;
export type AccessEventCallback = (event: AccessEvent) => void;
export type ErrorCallback = (error: Error) => void;

// Stream events from Argus daemon
export async function streamArgusEvents(
  onEvent: FileEventCallback,
  onError: ErrorCallback,
  options?: { watcherName?: string; namespace?: string }
): Promise<grpc.ClientReadableStream<FileEvent> | null> {
  try {
    const client = await getArgusdClient();
    if (!client) {
      onError(new Error('Failed to connect to Argusd'));
      return null;
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const stream = (client as any).StreamEvents({
      watcherName: options?.watcherName || '',
      namespace: options?.namespace || '',
      eventTypes: [],
    });

    stream.on('data', (data: FileEvent) => {
      onEvent(data);
    });

    stream.on('error', (error: Error) => {
      // UNAVAILABLE status means daemon is not running
      if ((error as grpc.ServiceError).code === grpc.status.UNAVAILABLE) {
        onError(new Error('Argusd not available'));
      } else {
        onError(error);
      }
    });

    stream.on('end', () => {
      console.log('Argus event stream ended');
    });

    return stream;
  } catch (error) {
    onError(error instanceof Error ? error : new Error(String(error)));
    return null;
  }
}

// Stream events from Janus daemon
export async function streamJanusEvents(
  onEvent: AccessEventCallback,
  onError: ErrorCallback,
  options?: { guardName?: string; namespace?: string; includeAllowed?: boolean }
): Promise<grpc.ClientReadableStream<AccessEvent> | null> {
  try {
    const client = await getJanusdClient();
    if (!client) {
      onError(new Error('Failed to connect to Janusd'));
      return null;
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const stream = (client as any).StreamAccessEvents({
      guardName: options?.guardName || '',
      namespace: options?.namespace || '',
      eventTypes: [],
      includeAllowed: options?.includeAllowed ?? true,
    });

    stream.on('data', (data: AccessEvent) => {
      onEvent(data);
    });

    stream.on('error', (error: Error) => {
      // UNAVAILABLE status means daemon is not running
      if ((error as grpc.ServiceError).code === grpc.status.UNAVAILABLE) {
        onError(new Error('Janusd not available'));
      } else {
        onError(error);
      }
    });

    stream.on('end', () => {
      console.log('Janus event stream ended');
    });

    return stream;
  } catch (error) {
    onError(error instanceof Error ? error : new Error(String(error)));
    return null;
  }
}

// Check daemon connectivity (to default or specific endpoint)
export async function checkDaemonConnectivity(argusdHost?: string, janusdHost?: string): Promise<{
  argusd: boolean;
  janusd: boolean;
  argusdError?: string;
  janusdError?: string;
}> {
  const result = {
    argusd: false,
    janusd: false,
    argusdError: undefined as string | undefined,
    janusdError: undefined as string | undefined,
  };

  // Check Argusd
  try {
    const client = await getArgusdClient(argusdHost);
    if (client) {
      // Try to get metrics as a connectivity check
      await new Promise<void>((resolve, reject) => {
        const deadline = new Date();
        deadline.setSeconds(deadline.getSeconds() + 5);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (client as any).GetMetrics(
          { watcherName: '' },
          { deadline },
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (err: any) => {
            if (err) reject(err);
            else resolve();
          }
        );
      });
      result.argusd = true;
    }
  } catch (error) {
    result.argusdError = error instanceof Error ? error.message : String(error);
  }

  // Check Janusd
  try {
    const client = await getJanusdClient(janusdHost);
    if (client) {
      await new Promise<void>((resolve, reject) => {
        const deadline = new Date();
        deadline.setSeconds(deadline.getSeconds() + 5);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (client as any).GetMetrics(
          { guardName: '' },
          { deadline },
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (err: any) => {
            if (err) reject(err);
            else resolve();
          }
        );
      });
      result.janusd = true;
    }
  } catch (error) {
    result.janusdError = error instanceof Error ? error.message : String(error);
  }

  return result;
}

// Stream events from a specific Argus daemon endpoint
export async function streamArgusEventsFromEndpoint(
  endpoint: string,
  onEvent: FileEventCallback,
  onError: ErrorCallback,
  options?: { watcherName?: string; namespace?: string }
): Promise<grpc.ClientReadableStream<FileEvent> | null> {
  try {
    const client = await getArgusdClient(endpoint);
    if (!client) {
      onError(new Error(`Failed to connect to Argusd at ${endpoint}`));
      return null;
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const stream = (client as any).StreamEvents({
      watcherName: options?.watcherName || '',
      namespace: options?.namespace || '',
      eventTypes: [],
    });

    stream.on('data', (data: FileEvent) => {
      onEvent(data);
    });

    stream.on('error', (error: Error) => {
      if ((error as grpc.ServiceError).code === grpc.status.UNAVAILABLE) {
        onError(new Error(`Argusd at ${endpoint} not available`));
      } else {
        onError(error);
      }
    });

    stream.on('end', () => {
      console.log(`Argus event stream from ${endpoint} ended`);
    });

    return stream;
  } catch (error) {
    onError(error instanceof Error ? error : new Error(String(error)));
    return null;
  }
}

// Stream events from a specific Janus daemon endpoint
export async function streamJanusEventsFromEndpoint(
  endpoint: string,
  onEvent: AccessEventCallback,
  onError: ErrorCallback,
  options?: { guardName?: string; namespace?: string; includeAllowed?: boolean }
): Promise<grpc.ClientReadableStream<AccessEvent> | null> {
  try {
    const client = await getJanusdClient(endpoint);
    if (!client) {
      onError(new Error(`Failed to connect to Janusd at ${endpoint}`));
      return null;
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const stream = (client as any).StreamAccessEvents({
      guardName: options?.guardName || '',
      namespace: options?.namespace || '',
      eventTypes: [],
      includeAllowed: options?.includeAllowed ?? true,
    });

    stream.on('data', (data: AccessEvent) => {
      onEvent(data);
    });

    stream.on('error', (error: Error) => {
      if ((error as grpc.ServiceError).code === grpc.status.UNAVAILABLE) {
        onError(new Error(`Janusd at ${endpoint} not available`));
      } else {
        onError(error);
      }
    });

    stream.on('end', () => {
      console.log(`Janus event stream from ${endpoint} ended`);
    });

    return stream;
  } catch (error) {
    onError(error instanceof Error ? error : new Error(String(error)));
    return null;
  }
}
