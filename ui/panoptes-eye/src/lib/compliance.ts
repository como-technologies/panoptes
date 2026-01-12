import type { ArgusWatcher } from '@/types/argus';
import type { JanusGuard } from '@/types/janus';
import type { ComplianceFramework, ComplianceCheck, ComplianceStatus, FrameworkResult, ComplianceResult } from '@/types/compliance';

// Helper to check if any watcher monitors specific paths
function hasWatcherForPath(watchers: ArgusWatcher[], pathPattern: string): boolean {
  return watchers.some(w =>
    !w.spec.paused &&
    w.spec.subjects.some(s =>
      s.paths.some(p => p.includes(pathPattern))
    )
  );
}

// Helper to check if any guard protects specific paths
function hasGuardForPath(guards: JanusGuard[], pathPattern: string): boolean {
  return guards.some(g =>
    !g.spec.paused &&
    g.spec.subjects.some(s =>
      (s.allow?.some(p => p.includes(pathPattern)) ||
       s.deny?.some(p => p.includes(pathPattern)))
    )
  );
}

// Helper to check if enforcing mode guards exist
function hasEnforcingGuards(guards: JanusGuard[]): boolean {
  return guards.some(g => g.spec.enforcing && !g.spec.paused);
}

// Helper to check if audit-enabled guards exist
function hasAuditGuards(guards: JanusGuard[]): boolean {
  return guards.some(g =>
    !g.spec.paused &&
    g.spec.subjects.some(s => s.audit === true)
  );
}

// PCI-DSS Framework Checks
const pciDssChecks: ComplianceCheck[] = [
  {
    id: 'pci-10.5.5',
    name: 'File Integrity Monitoring',
    description: 'Use file integrity monitoring or change-detection software on logs to ensure that existing log data cannot be changed without generating alerts.',
    requirement: 'PCI-DSS 10.5.5',
    framework: 'pci-dss',
    evaluate: (watchers) => {
      if (watchers.length === 0) return 'fail';
      const hasLogWatcher = hasWatcherForPath(watchers, '/var/log');
      return hasLogWatcher ? 'pass' : 'warning';
    },
  },
  {
    id: 'pci-10.2',
    name: 'Audit Trail Logging',
    description: 'Implement automated audit trails for all system components to reconstruct events.',
    requirement: 'PCI-DSS 10.2',
    framework: 'pci-dss',
    evaluate: (_, guards) => {
      if (guards.length === 0) return 'fail';
      return hasAuditGuards(guards) ? 'pass' : 'warning';
    },
  },
  {
    id: 'pci-10.6',
    name: 'Security Alert Review',
    description: 'Review logs and security events for all system components to identify anomalies or suspicious activity.',
    requirement: 'PCI-DSS 10.6',
    framework: 'pci-dss',
    evaluate: (watchers, guards) => {
      if (watchers.length === 0 && guards.length === 0) return 'fail';
      const hasActiveWatchers = watchers.some(w => !w.spec.paused);
      const hasActiveGuards = guards.some(g => !g.spec.paused);
      return hasActiveWatchers || hasActiveGuards ? 'pass' : 'warning';
    },
  },
  {
    id: 'pci-7.1',
    name: 'Access Control Enforcement',
    description: 'Limit access to system components and cardholder data to only those individuals whose job requires such access.',
    requirement: 'PCI-DSS 7.1',
    framework: 'pci-dss',
    evaluate: (_, guards) => {
      if (guards.length === 0) return 'fail';
      return hasEnforcingGuards(guards) ? 'pass' : 'warning';
    },
  },
  {
    id: 'pci-11.5',
    name: 'Change Detection Mechanism',
    description: 'Deploy a change-detection mechanism to alert personnel to unauthorized modification of critical files.',
    requirement: 'PCI-DSS 11.5',
    framework: 'pci-dss',
    evaluate: (watchers) => {
      if (watchers.length === 0) return 'fail';
      const hasConfigWatcher = hasWatcherForPath(watchers, '/etc');
      return hasConfigWatcher ? 'pass' : 'warning';
    },
  },
];

// HIPAA Framework Checks
const hipaaChecks: ComplianceCheck[] = [
  {
    id: 'hipaa-164.312-b',
    name: 'Audit Controls',
    description: 'Implement hardware, software, and/or procedural mechanisms that record and examine activity in information systems.',
    requirement: 'HIPAA 164.312(b)',
    framework: 'hipaa',
    evaluate: (watchers, guards) => {
      if (watchers.length === 0 && guards.length === 0) return 'fail';
      return (watchers.length > 0 || guards.length > 0) ? 'pass' : 'fail';
    },
  },
  {
    id: 'hipaa-164.312-c-1',
    name: 'Data Integrity',
    description: 'Implement policies and procedures to protect electronic protected health information from improper alteration or destruction.',
    requirement: 'HIPAA 164.312(c)(1)',
    framework: 'hipaa',
    evaluate: (watchers) => {
      if (watchers.length === 0) return 'fail';
      const hasDataWatcher = watchers.some(w =>
        !w.spec.paused &&
        w.spec.subjects.some(s => s.events.includes('modify') || s.events.includes('delete'))
      );
      return hasDataWatcher ? 'pass' : 'warning';
    },
  },
  {
    id: 'hipaa-164.312-d',
    name: 'Person or Entity Authentication',
    description: 'Implement procedures to verify that a person or entity seeking access to ePHI is the one claimed.',
    requirement: 'HIPAA 164.312(d)',
    framework: 'hipaa',
    evaluate: (_, guards) => {
      if (guards.length === 0) return 'warning';
      return hasEnforcingGuards(guards) ? 'pass' : 'warning';
    },
  },
  {
    id: 'hipaa-164.308-a-1',
    name: 'Security Management Process',
    description: 'Implement policies and procedures to prevent, detect, contain, and correct security violations.',
    requirement: 'HIPAA 164.308(a)(1)',
    framework: 'hipaa',
    evaluate: (watchers, guards) => {
      const hasActiveWatchers = watchers.some(w => !w.spec.paused);
      const hasActiveGuards = guards.some(g => !g.spec.paused);
      if (!hasActiveWatchers && !hasActiveGuards) return 'fail';
      return hasActiveWatchers && hasActiveGuards ? 'pass' : 'warning';
    },
  },
];

// SOC2 Framework Checks
const soc2Checks: ComplianceCheck[] = [
  {
    id: 'soc2-cc6.1',
    name: 'Logical Access Security',
    description: 'The entity implements logical access security software, infrastructure, and architectures over protected information assets.',
    requirement: 'SOC2 CC6.1',
    framework: 'soc2',
    evaluate: (_, guards) => {
      if (guards.length === 0) return 'fail';
      return hasEnforcingGuards(guards) ? 'pass' : 'warning';
    },
  },
  {
    id: 'soc2-cc6.2',
    name: 'Access Authorization',
    description: 'Prior to issuing system credentials and granting system access, the entity registers and authorizes new internal and external users.',
    requirement: 'SOC2 CC6.2',
    framework: 'soc2',
    evaluate: (_, guards) => {
      if (guards.length === 0) return 'warning';
      const hasGuardWithDeny = guards.some(g =>
        !g.spec.paused &&
        g.spec.subjects.some(s => s.deny && s.deny.length > 0)
      );
      return hasGuardWithDeny ? 'pass' : 'warning';
    },
  },
  {
    id: 'soc2-cc7.2',
    name: 'System Monitoring',
    description: 'The entity monitors system components and the operation of those components for anomalies.',
    requirement: 'SOC2 CC7.2',
    framework: 'soc2',
    evaluate: (watchers) => {
      if (watchers.length === 0) return 'fail';
      const hasActiveWatchers = watchers.some(w => !w.spec.paused);
      return hasActiveWatchers ? 'pass' : 'warning';
    },
  },
  {
    id: 'soc2-cc7.3',
    name: 'Incident Detection',
    description: 'The entity evaluates security events to determine whether they could or have resulted in a failure of the entity to meet its objectives.',
    requirement: 'SOC2 CC7.3',
    framework: 'soc2',
    evaluate: (watchers, guards) => {
      const hasActiveMonitoring = watchers.some(w => !w.spec.paused) || guards.some(g => !g.spec.paused);
      return hasActiveMonitoring ? 'pass' : 'fail';
    },
  },
];

// CIS Kubernetes Benchmark Checks
const cisChecks: ComplianceCheck[] = [
  {
    id: 'cis-1.1.1',
    name: 'API Server Pod Specification',
    description: 'Ensure that the API server pod specification file permissions are set to 644 or more restrictive.',
    requirement: 'CIS 1.1.1',
    framework: 'cis',
    evaluate: (watchers) => {
      const hasEtcKubernetesWatch = hasWatcherForPath(watchers, '/etc/kubernetes');
      return hasEtcKubernetesWatch ? 'pass' : 'warning';
    },
  },
  {
    id: 'cis-5.7.1',
    name: 'Namespace Creation',
    description: 'Create administrative boundaries between resources using namespaces.',
    requirement: 'CIS 5.7.1',
    framework: 'cis',
    evaluate: (watchers, guards) => {
      // Check if there are resources in multiple namespaces
      const watcherNamespaces = watchers.map(w => w.metadata.namespace);
      const guardNamespaces = guards.map(g => g.metadata.namespace);
      const allNamespaces = [...watcherNamespaces, ...guardNamespaces];
      const uniqueNamespaces = allNamespaces.filter((ns, i) => allNamespaces.indexOf(ns) === i);
      return uniqueNamespaces.length > 1 ? 'pass' : 'warning';
    },
  },
  {
    id: 'cis-5.1.6',
    name: 'Service Account Token',
    description: 'Ensure that Service Account Tokens are only mounted where necessary.',
    requirement: 'CIS 5.1.6',
    framework: 'cis',
    evaluate: (_, guards) => {
      const hasTokenGuard = hasGuardForPath(guards, '/var/run/secrets');
      return hasTokenGuard ? 'pass' : 'warning';
    },
  },
  {
    id: 'cis-4.2.4',
    name: 'Kubelet Read-Only Port',
    description: 'Ensure that the --read-only-port argument is set to 0.',
    requirement: 'CIS 4.2.4',
    framework: 'cis',
    evaluate: (watchers) => {
      const hasKubeletConfig = hasWatcherForPath(watchers, 'kubelet');
      return hasKubeletConfig ? 'pass' : 'unknown';
    },
  },
];

// Framework Definitions
export const complianceFrameworks: ComplianceFramework[] = [
  {
    id: 'pci-dss',
    name: 'PCI-DSS',
    description: 'Payment Card Industry Data Security Standard - Requirements for organizations handling credit card data.',
    checks: pciDssChecks,
  },
  {
    id: 'hipaa',
    name: 'HIPAA',
    description: 'Health Insurance Portability and Accountability Act - Security requirements for protected health information.',
    checks: hipaaChecks,
  },
  {
    id: 'soc2',
    name: 'SOC 2',
    description: 'Service Organization Control 2 - Trust Services Criteria for service organizations.',
    checks: soc2Checks,
  },
  {
    id: 'cis',
    name: 'CIS Kubernetes',
    description: 'Center for Internet Security Kubernetes Benchmark - Security configuration guidelines.',
    checks: cisChecks,
  },
];

// Evaluate a single framework
export function evaluateFramework(
  framework: ComplianceFramework,
  watchers: ArgusWatcher[],
  guards: JanusGuard[]
): FrameworkResult {
  const results: ComplianceResult[] = framework.checks.map(check => ({
    check,
    status: check.evaluate(watchers, guards),
  }));

  const passCount = results.filter(r => r.status === 'pass').length;
  const failCount = results.filter(r => r.status === 'fail').length;
  const warningCount = results.filter(r => r.status === 'warning').length;
  const unknownCount = results.filter(r => r.status === 'unknown').length;

  const total = results.length;
  const score = total > 0 ? Math.round((passCount / total) * 100) : 0;

  return {
    framework,
    results,
    passCount,
    failCount,
    warningCount,
    unknownCount,
    score,
  };
}

// Evaluate all frameworks
export function evaluateAllFrameworks(
  watchers: ArgusWatcher[],
  guards: JanusGuard[]
): FrameworkResult[] {
  return complianceFrameworks.map(framework =>
    evaluateFramework(framework, watchers, guards)
  );
}

// Get overall compliance score across all frameworks
export function getOverallScore(frameworkResults: FrameworkResult[]): number {
  if (frameworkResults.length === 0) return 0;
  const totalScore = frameworkResults.reduce((sum, r) => sum + r.score, 0);
  return Math.round(totalScore / frameworkResults.length);
}
