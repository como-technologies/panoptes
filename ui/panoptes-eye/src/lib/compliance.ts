import type { ArgusWatcher, ArgusSubject } from '@/types/argus';
import type { JanusGuard, JanusSubject } from '@/types/janus';
import type { ComplianceFramework, ComplianceCheck, ComplianceStatus, FrameworkResult, ComplianceResult, RemediationAction } from '@/types/compliance';

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
    remediation: 'Create an ArgusWatcher monitoring /var/log with events: [modify, delete]. Apply the PCI-DSS template: kubectl apply -f docs/compliance-templates/pci-dss.yaml',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'pci-log-monitoring',
      subjects: [{
        paths: ['/var/log'],
        events: ['modify', 'delete'],
        recursive: true,
        tags: { requirement: '10.5.5', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'pci-dss/scope': 'in-scope' },
    },
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
    remediation: 'Create a JanusGuard with audit: true on subjects to enable access logging. Apply the PCI-DSS template for comprehensive audit coverage.',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'pci-audit-trail',
      subjects: [{
        allow: ['/'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: '10.2', severity: 'high' },
      }] as JanusSubject[],
      enforcing: false,
      suggestedSelector: { 'pci-dss/scope': 'in-scope' },
    },
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
    remediation: 'Ensure at least one ArgusWatcher or JanusGuard is active (not paused). Check the Events page for real-time monitoring.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'pci-security-alerts',
      subjects: [{
        paths: ['/var/log', '/etc'],
        events: ['modify', 'create', 'delete'],
        recursive: true,
        tags: { requirement: '10.6', severity: 'medium' },
      }] as ArgusSubject[],
      suggestedSelector: { 'pci-dss/scope': 'in-scope' },
    },
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
    remediation: 'Create a JanusGuard with enforcing: true to actively block unauthorized access. First test with enforcing: false, then enable after validating no false positives.',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'pci-access-control',
      subjects: [{
        deny: ['/etc/shadow', '/root/.ssh'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: '7.1', severity: 'critical' },
      }] as JanusSubject[],
      enforcing: false, // Start in audit mode for safety
      suggestedSelector: { 'pci-dss/scope': 'in-scope' },
    },
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
    remediation: 'Create an ArgusWatcher monitoring /etc with events: [modify, create, delete]. Include /etc/passwd, /etc/shadow, /etc/sudoers for user account changes.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'pci-change-detection',
      subjects: [{
        paths: ['/etc/passwd', '/etc/shadow', '/etc/group', '/etc/sudoers'],
        events: ['modify', 'delete', 'attrib'],
        tags: { requirement: '11.5', severity: 'critical' },
      }, {
        paths: ['/etc'],
        events: ['modify', 'create', 'delete'],
        recursive: true,
        maxDepth: 2,
        tags: { requirement: '11.5', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'pci-dss/scope': 'in-scope' },
    },
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
    remediation: 'Deploy at least one ArgusWatcher or JanusGuard to monitor ePHI-containing systems. Apply the HIPAA template: kubectl apply -f docs/compliance-templates/hipaa.yaml',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'hipaa-audit-controls',
      subjects: [{
        paths: ['/var/log', '/etc'],
        events: ['modify', 'delete', 'create'],
        recursive: true,
        tags: { requirement: '164.312(b)', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'hipaa/scope': 'ephi' },
    },
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
    remediation: 'Create an ArgusWatcher with events: [modify, delete] on ePHI directories. Monitor /var/log for audit log integrity.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'hipaa-data-integrity',
      subjects: [{
        paths: ['/data', '/app/data'],
        events: ['modify', 'delete'],
        recursive: true,
        tags: { requirement: '164.312(c)(1)', severity: 'critical' },
      }, {
        paths: ['/var/log'],
        events: ['modify', 'delete'],
        recursive: true,
        tags: { requirement: '164.312(c)(1)', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'hipaa/scope': 'ephi' },
    },
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
    remediation: 'Create a JanusGuard with enforcing: true to control access to ePHI. Use deny rules for sensitive credential files like /etc/shadow.',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'hipaa-authentication',
      subjects: [{
        deny: ['/etc/shadow', '/etc/passwd'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: '164.312(d)', severity: 'critical' },
      }] as JanusSubject[],
      enforcing: false, // Start in audit mode
      suggestedSelector: { 'hipaa/scope': 'ephi' },
    },
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
    remediation: 'Deploy both an ArgusWatcher (detection) and JanusGuard (prevention). Ensure neither is paused for continuous protection.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'hipaa-security-management',
      subjects: [{
        paths: ['/etc', '/var/log', '/usr/bin', '/usr/sbin'],
        events: ['modify', 'create', 'delete'],
        recursive: true,
        tags: { requirement: '164.308(a)(1)', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'hipaa/scope': 'ephi' },
    },
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
    remediation: 'Create a JanusGuard with enforcing: true to implement active access controls. Apply the SOC 2 template: kubectl apply -f docs/compliance-templates/soc2.yaml',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'soc2-access-security',
      subjects: [{
        deny: ['/etc/shadow', '/etc/sudoers'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: 'CC6.1', severity: 'critical' },
      }] as JanusSubject[],
      enforcing: false, // Start in audit mode
      suggestedSelector: { 'soc2/scope': 'in-scope' },
    },
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
    remediation: 'Create a JanusGuard with deny rules for sensitive paths like /etc/shadow, /root/.ssh to enforce access authorization.',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'soc2-access-authorization',
      subjects: [{
        deny: ['/etc/shadow', '/root/.ssh', '/var/run/secrets'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: 'CC6.2', severity: 'critical' },
      }] as JanusSubject[],
      enforcing: false,
      suggestedSelector: { 'soc2/scope': 'in-scope' },
    },
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
    remediation: 'Create an active ArgusWatcher (paused: false) to monitor system files. Monitor /usr/bin, /var/log for anomaly detection.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'soc2-system-monitoring',
      subjects: [{
        paths: ['/usr/bin', '/usr/sbin', '/var/log'],
        events: ['modify', 'create', 'delete'],
        recursive: true,
        tags: { requirement: 'CC7.2', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'soc2/scope': 'in-scope' },
    },
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
    remediation: 'Ensure at least one ArgusWatcher or JanusGuard is active. Use the Events page to review security incidents.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'soc2-incident-detection',
      subjects: [{
        paths: ['/etc', '/var/log'],
        events: ['all'],
        recursive: true,
        tags: { requirement: 'CC7.3', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'soc2/scope': 'in-scope' },
    },
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
    remediation: 'Create an ArgusWatcher monitoring /etc/kubernetes with events: [modify, attrib]. Apply the CIS template: kubectl apply -f docs/compliance-templates/cis-kubernetes.yaml',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'cis-api-server-config',
      subjects: [{
        paths: ['/etc/kubernetes'],
        events: ['modify', 'attrib', 'create', 'delete'],
        recursive: true,
        tags: { requirement: 'CIS 1.1.1', severity: 'critical' },
      }] as ArgusSubject[],
      suggestedSelector: { 'cis/scope': 'control-plane' },
    },
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
    remediation: 'Deploy ArgusWatchers and JanusGuards in multiple namespaces to establish administrative boundaries.',
    // No remediationAction - this is an organizational requirement, not a resource creation
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
    remediation: 'Create a JanusGuard with deny rules for /var/run/secrets and autoAllowOwner: true to control service account token access.',
    remediationAction: {
      resourceType: 'JanusGuard',
      suggestedName: 'cis-token-protection',
      subjects: [{
        deny: ['/var/run/secrets/kubernetes.io'],
        events: ['access', 'open'],
        audit: true,
        tags: { requirement: 'CIS 5.1.6', severity: 'high' },
      }] as JanusSubject[],
      enforcing: false,
      suggestedSelector: { 'cis/scope': 'workloads' },
    },
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
    remediation: 'Create an ArgusWatcher monitoring kubelet configuration files to detect changes to read-only port settings.',
    remediationAction: {
      resourceType: 'ArgusWatcher',
      suggestedName: 'cis-kubelet-config',
      subjects: [{
        paths: ['/var/lib/kubelet', '/etc/kubernetes/kubelet.conf'],
        events: ['modify', 'attrib'],
        tags: { requirement: 'CIS 4.2.4', severity: 'high' },
      }] as ArgusSubject[],
      suggestedSelector: { 'cis/scope': 'nodes' },
    },
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
