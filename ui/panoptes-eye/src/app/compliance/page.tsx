'use client';

import { useState, useMemo } from 'react';
import { CheckCircle, XCircle, AlertTriangle, HelpCircle, RefreshCw, ChevronDown, ChevronRight, Download, FileText, Copy, Check, Wrench } from 'lucide-react';
import { useWatchers, useGuards } from '@/hooks/useK8s';
import { complianceFrameworks, evaluateAllFrameworks, getOverallScore } from '@/lib/compliance';
import type { ComplianceStatus, FrameworkResult, ComplianceCheck } from '@/types/compliance';
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Dialog, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';
import { ResourceCreationDialog, type ResourceCreationConfig } from '@/components/resources/ResourceCreationDialog';

// Mapping of framework IDs to their template files
const frameworkTemplates: Record<string, { file: string; description: string }> = {
  'pci-dss': {
    file: 'pci-dss.yaml',
    description: 'PCI-DSS 3.2.1/4.0 monitoring for payment card data environments',
  },
  'hipaa': {
    file: 'hipaa.yaml',
    description: 'HIPAA Security Rule monitoring for healthcare environments',
  },
  'soc2': {
    file: 'soc2.yaml',
    description: 'SOC 2 Trust Services Criteria monitoring',
  },
  'cis': {
    file: 'cis-kubernetes.yaml',
    description: 'CIS Kubernetes Benchmark v1.8 monitoring',
  },
};

function StatusIcon({ status }: { status: ComplianceStatus }) {
  switch (status) {
    case 'pass':
      return <CheckCircle className="h-5 w-5 text-green-500" />;
    case 'fail':
      return <XCircle className="h-5 w-5 text-red-500" />;
    case 'warning':
      return <AlertTriangle className="h-5 w-5 text-amber-500" />;
    default:
      return <HelpCircle className="h-5 w-5 text-gray-400" />;
  }
}

function StatusBadge({ status }: { status: ComplianceStatus }) {
  const variants: Record<ComplianceStatus, 'active' | 'error' | 'warning' | 'default'> = {
    pass: 'active',
    fail: 'error',
    warning: 'warning',
    unknown: 'default',
  };

  const labels: Record<ComplianceStatus, string> = {
    pass: 'Pass',
    fail: 'Fail',
    warning: 'Warning',
    unknown: 'Unknown',
  };

  return <Badge variant={variants[status]}>{labels[status]}</Badge>;
}

function ScoreRing({ score, size = 120 }: { score: number; size?: number }) {
  // Scale stroke width and font size proportionally with ring size
  const strokeWidth = Math.max(2, Math.round(size / 15));
  const fontSize = Math.max(10, Math.round(size / 5));
  const radius = (size - strokeWidth * 2) / 2;
  const circumference = 2 * Math.PI * radius;
  const progress = (score / 100) * circumference;
  const offset = circumference - progress;

  const getColor = (s: number) => {
    if (s >= 80) return 'text-green-500';
    if (s >= 60) return 'text-amber-500';
    return 'text-red-500';
  };

  return (
    <div className="relative" style={{ width: size, height: size }}>
      <svg className="transform -rotate-90" width={size} height={size}>
        <circle
          className="text-gray-200 dark:text-gray-700"
          strokeWidth={strokeWidth}
          stroke="currentColor"
          fill="transparent"
          r={radius}
          cx={size / 2}
          cy={size / 2}
        />
        <circle
          className={getColor(score)}
          strokeWidth={strokeWidth}
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          strokeLinecap="round"
          stroke="currentColor"
          fill="transparent"
          r={radius}
          cx={size / 2}
          cy={size / 2}
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <span className={`font-bold ${getColor(score)}`} style={{ fontSize }}>{score}%</span>
      </div>
    </div>
  );
}

function FrameworkCard({ result, isExpanded, onToggle, onViewTemplate, onApplyFix }: {
  result: FrameworkResult;
  isExpanded: boolean;
  onToggle: () => void;
  onViewTemplate: () => void;
  onApplyFix: (check: ComplianceCheck) => void;
}) {
  return (
    <Card>
      <CardHeader
        className="cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
        onClick={onToggle}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <ScoreRing score={result.score} size={60} />
            <div>
              <CardTitle className="text-lg">{result.framework.name}</CardTitle>
              <CardDescription className="mt-1">
                {result.passCount} passed, {result.failCount} failed, {result.warningCount} warnings
              </CardDescription>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={(e) => {
                e.stopPropagation();
                onViewTemplate();
              }}
            >
              <FileText className="h-4 w-4 mr-1" />
              Template
            </Button>
            {isExpanded ? (
              <ChevronDown className="h-5 w-5 text-gray-400" />
            ) : (
              <ChevronRight className="h-5 w-5 text-gray-400" />
            )}
          </div>
        </div>
      </CardHeader>
      {isExpanded && (
        <CardContent className="pt-0">
          <div className="border-t pt-4 space-y-3">
            {result.results.map((r) => (
              <div
                key={r.check.id}
                className="flex items-start gap-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
              >
                <StatusIcon status={r.status} />
                <div className="flex-1">
                  <div className="flex items-center justify-between">
                    <p className="font-medium">{r.check.name}</p>
                    <StatusBadge status={r.status} />
                  </div>
                  <p className="text-sm text-gray-500 mt-1">{r.check.description}</p>
                  <p className="text-xs text-blue-500 mt-1">{r.check.requirement}</p>
                  {(r.status === 'fail' || r.status === 'warning') && (
                    <div className="mt-2 p-2 bg-amber-50 dark:bg-amber-900/20 rounded border border-amber-200 dark:border-amber-800">
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1">
                          <p className="text-xs font-medium text-amber-800 dark:text-amber-200">Remediation:</p>
                          <p className="text-xs text-amber-700 dark:text-amber-300 mt-0.5">{r.check.remediation}</p>
                        </div>
                        {r.check.remediationAction && (
                          <Button
                            variant="outline"
                            size="sm"
                            className="shrink-0 text-xs h-7 px-2"
                            onClick={(e) => {
                              e.stopPropagation();
                              onApplyFix(r.check);
                            }}
                          >
                            <Wrench className="h-3 w-3 mr-1" />
                            Apply Fix
                          </Button>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      )}
    </Card>
  );
}

function FrameworkFilter({
  frameworks,
  selected,
  onChange,
}: {
  frameworks: typeof complianceFrameworks;
  selected: string[];
  onChange: (ids: string[]) => void;
}) {
  const toggleFramework = (id: string) => {
    if (selected.includes(id)) {
      onChange(selected.filter(s => s !== id));
    } else {
      onChange([...selected, id]);
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      {frameworks.map((f) => (
        <Button
          key={f.id}
          variant={selected.includes(f.id) ? 'primary' : 'outline'}
          size="sm"
          onClick={() => toggleFramework(f.id)}
        >
          {f.name}
        </Button>
      ))}
    </div>
  );
}

export default function CompliancePage() {
  const { data: watchers, isLoading: watchersLoading, refetch: refetchWatchers } = useWatchers();
  const { data: guards, isLoading: guardsLoading, refetch: refetchGuards } = useGuards();

  const [selectedFrameworks, setSelectedFrameworks] = useState<string[]>(
    complianceFrameworks.map(f => f.id)
  );
  const [expandedFramework, setExpandedFramework] = useState<string | null>(null);
  const [templateDialog, setTemplateDialog] = useState<{ open: boolean; frameworkId: string | null }>({
    open: false,
    frameworkId: null,
  });
  const [copied, setCopied] = useState(false);
  const [remediationDialog, setRemediationDialog] = useState<{
    open: boolean;
    config: ResourceCreationConfig | null;
  }>({ open: false, config: null });

  const isLoading = watchersLoading || guardsLoading;

  const frameworkResults = useMemo(() => {
    if (!watchers || !guards) return [];
    return evaluateAllFrameworks(watchers, guards);
  }, [watchers, guards]);

  const filteredResults = frameworkResults.filter(r =>
    selectedFrameworks.includes(r.framework.id)
  );

  const overallScore = getOverallScore(filteredResults);

  const handleRefresh = () => {
    refetchWatchers();
    refetchGuards();
  };

  const openTemplateDialog = (frameworkId: string) => {
    setTemplateDialog({ open: true, frameworkId });
    setCopied(false);
  };

  const closeTemplateDialog = () => {
    setTemplateDialog({ open: false, frameworkId: null });
    setCopied(false);
  };

  const getKubectlCommand = (frameworkId: string) => {
    const template = frameworkTemplates[frameworkId];
    if (!template) return '';
    return `kubectl apply -f https://raw.githubusercontent.com/CoMo-Technologies/panoptes/main/docs/compliance-templates/${template.file}`;
  };

  const copyToClipboard = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleApplyFix = (check: ComplianceCheck) => {
    if (!check.remediationAction) return;
    const action = check.remediationAction;
    setRemediationDialog({
      open: true,
      config: {
        resourceType: action.resourceType,
        name: action.suggestedName,
        selector: action.suggestedSelector,
        subjects: action.subjects,
        enforcing: action.enforcing,
      },
    });
  };

  const closeRemediationDialog = () => {
    setRemediationDialog({ open: false, config: null });
  };

  const handleRemediationSuccess = () => {
    // Refresh data to update compliance status
    refetchWatchers();
    refetchGuards();
  };

  const exportReport = (format: 'json' | 'csv') => {
    const timestamp = new Date().toISOString();

    if (format === 'json') {
      const report = {
        generatedAt: timestamp,
        overallScore,
        frameworks: filteredResults.map((r) => ({
          id: r.framework.id,
          name: r.framework.name,
          score: r.score,
          passCount: r.passCount,
          failCount: r.failCount,
          warningCount: r.warningCount,
          checks: r.results.map((check) => ({
            id: check.check.id,
            name: check.check.name,
            requirement: check.check.requirement,
            status: check.status,
            remediation: check.status !== 'pass' ? check.check.remediation : undefined,
          })),
        })),
      };

      const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `compliance-report-${timestamp.split('T')[0]}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } else {
      const rows = [
        ['Framework', 'Check ID', 'Check Name', 'Requirement', 'Status', 'Remediation'],
      ];

      for (const result of filteredResults) {
        for (const check of result.results) {
          rows.push([
            result.framework.name,
            check.check.id,
            check.check.name,
            check.check.requirement,
            check.status,
            check.status !== 'pass' ? check.check.remediation : '',
          ]);
        }
      }

      const csv = rows.map((row) => row.map((cell) => `"${cell.replace(/"/g, '""')}"`).join(',')).join('\n');
      const blob = new Blob([csv], { type: 'text/csv' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `compliance-report-${timestamp.split('T')[0]}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    }
  };

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Compliance</h1>
          <p className="text-gray-500 dark:text-gray-400">
            Security compliance status against common frameworks
          </p>
        </div>
        <Card>
          <CardContent className="p-6">
            <div className="flex items-center justify-center">
              <Skeleton className="h-32 w-32 rounded-full" />
            </div>
            <div className="mt-4 space-y-2">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Compliance</h1>
          <p className="text-gray-500 dark:text-gray-400">
            Security compliance status against common frameworks
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={() => exportReport('json')}>
            <Download className="h-4 w-4 mr-2" />
            JSON
          </Button>
          <Button variant="outline" onClick={() => exportReport('csv')}>
            <Download className="h-4 w-4 mr-2" />
            CSV
          </Button>
          <Button variant="outline" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardContent className="p-6">
          <div className="flex flex-col md:flex-row items-center gap-8">
            <ScoreRing score={overallScore} size={140} />
            <div className="flex-1">
              <h2 className="text-xl font-semibold mb-2">Overall Compliance Score</h2>
              <p className="text-gray-500 dark:text-gray-400 mb-4">
                Based on {filteredResults.length} selected frameworks with{' '}
                {filteredResults.reduce((sum, r) => sum + r.results.length, 0)} total checks
              </p>
              <div className="flex flex-wrap gap-4 text-sm">
                <div className="flex items-center gap-2">
                  <CheckCircle className="h-4 w-4 text-green-500" />
                  <span>{filteredResults.reduce((sum, r) => sum + r.passCount, 0)} Passed</span>
                </div>
                <div className="flex items-center gap-2">
                  <XCircle className="h-4 w-4 text-red-500" />
                  <span>{filteredResults.reduce((sum, r) => sum + r.failCount, 0)} Failed</span>
                </div>
                <div className="flex items-center gap-2">
                  <AlertTriangle className="h-4 w-4 text-amber-500" />
                  <span>{filteredResults.reduce((sum, r) => sum + r.warningCount, 0)} Warnings</span>
                </div>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <div>
        <h3 className="text-sm font-medium mb-3">Filter Frameworks</h3>
        <FrameworkFilter
          frameworks={complianceFrameworks}
          selected={selectedFrameworks}
          onChange={setSelectedFrameworks}
        />
      </div>

      <div className="space-y-4">
        {filteredResults.map((result) => (
          <FrameworkCard
            key={result.framework.id}
            result={result}
            isExpanded={expandedFramework === result.framework.id}
            onToggle={() => setExpandedFramework(
              expandedFramework === result.framework.id ? null : result.framework.id
            )}
            onViewTemplate={() => openTemplateDialog(result.framework.id)}
            onApplyFix={handleApplyFix}
          />
        ))}
      </div>

      {filteredResults.length === 0 && (
        <Card className="p-8 text-center">
          <p className="text-gray-500">Select at least one framework to view compliance status</p>
        </Card>
      )}

      {/* Template Dialog */}
      <Dialog open={templateDialog.open} onClose={closeTemplateDialog}>
        <DialogHeader>
          <DialogTitle>Apply Compliance Template</DialogTitle>
          <DialogDescription>
            {templateDialog.frameworkId && frameworkTemplates[templateDialog.frameworkId]?.description}
          </DialogDescription>
        </DialogHeader>
        <div className="mt-4">
          <p className="text-sm font-medium mb-2">Apply with kubectl:</p>
          <div className="relative">
            <pre className="bg-gray-100 dark:bg-gray-900 p-3 rounded text-xs overflow-x-auto">
              {templateDialog.frameworkId && getKubectlCommand(templateDialog.frameworkId)}
            </pre>
            <Button
              variant="ghost"
              size="sm"
              className="absolute top-1 right-1"
              onClick={() => templateDialog.frameworkId && copyToClipboard(getKubectlCommand(templateDialog.frameworkId))}
            >
              {copied ? (
                <Check className="h-4 w-4 text-green-500" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </Button>
          </div>
          <p className="text-xs text-gray-500 mt-3">
            After applying, label your pods to start monitoring:
          </p>
          <pre className="bg-gray-100 dark:bg-gray-900 p-2 rounded text-xs mt-1">
            kubectl label pods -l app=myapp {templateDialog.frameworkId}/scope=in-scope
          </pre>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={closeTemplateDialog}>
            Close
          </Button>
        </DialogFooter>
      </Dialog>

      {/* Remediation Dialog */}
      <ResourceCreationDialog
        open={remediationDialog.open}
        onClose={closeRemediationDialog}
        initialConfig={remediationDialog.config || undefined}
        onSuccess={handleRemediationSuccess}
      />
    </div>
  );
}
