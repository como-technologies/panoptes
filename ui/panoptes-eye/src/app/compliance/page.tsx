'use client';

import { useState, useMemo } from 'react';
import { CheckCircle, XCircle, AlertTriangle, HelpCircle, RefreshCw, ChevronDown, ChevronRight } from 'lucide-react';
import { useWatchers, useGuards } from '@/hooks/useK8s';
import { complianceFrameworks, evaluateAllFrameworks, getOverallScore } from '@/lib/compliance';
import type { ComplianceStatus, FrameworkResult } from '@/types/compliance';
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';

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

function FrameworkCard({ result, isExpanded, onToggle }: {
  result: FrameworkResult;
  isExpanded: boolean;
  onToggle: () => void;
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
        <Button variant="outline" onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
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
          />
        ))}
      </div>

      {filteredResults.length === 0 && (
        <Card className="p-8 text-center">
          <p className="text-gray-500">Select at least one framework to view compliance status</p>
        </Card>
      )}
    </div>
  );
}
