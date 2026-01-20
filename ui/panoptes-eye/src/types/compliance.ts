import type { ArgusWatcher, ArgusSubject } from './argus';
import type { JanusGuard, JanusSubject } from './janus';

export type ComplianceStatus = 'pass' | 'fail' | 'warning' | 'unknown';

/** Resource type for remediation actions */
export type RemediationResourceType = 'ArgusWatcher' | 'JanusGuard';

/** Structured configuration for one-click remediation */
export interface RemediationAction {
  /** Type of resource to create */
  resourceType: RemediationResourceType;
  /** Suggested name for the resource */
  suggestedName: string;
  /** Pre-configured subjects (paths/events for Argus, allow/deny for Janus) */
  subjects: ArgusSubject[] | JanusSubject[];
  /** For JanusGuard: whether to enable enforcing mode */
  enforcing?: boolean;
  /** Suggested selector labels for targeting pods */
  suggestedSelector?: Record<string, string>;
}

export interface ComplianceCheck {
  id: string;
  name: string;
  description: string;
  requirement: string;
  framework: string;
  remediation: string;
  /** Structured remediation action for one-click fix */
  remediationAction?: RemediationAction;
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
