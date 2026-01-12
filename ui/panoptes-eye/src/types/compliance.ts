import type { ArgusWatcher } from './argus';
import type { JanusGuard } from './janus';

export type ComplianceStatus = 'pass' | 'fail' | 'warning' | 'unknown';

export interface ComplianceCheck {
  id: string;
  name: string;
  description: string;
  requirement: string;
  framework: string;
  evaluate: (watchers: ArgusWatcher[], guards: JanusGuard[]) => ComplianceStatus;
}

export interface ComplianceFramework {
  id: string;
  name: string;
  description: string;
  checks: ComplianceCheck[];
}

export interface ComplianceResult {
  check: ComplianceCheck;
  status: ComplianceStatus;
}

export interface FrameworkResult {
  framework: ComplianceFramework;
  results: ComplianceResult[];
  passCount: number;
  failCount: number;
  warningCount: number;
  unknownCount: number;
  score: number;
}
