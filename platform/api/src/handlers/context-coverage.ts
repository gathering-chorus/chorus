/* eslint-disable security/detect-object-injection -- FileCoverage indexed by validated keys. */
/**
 * GET /api/chorus/context/coverage?domain={d} (#2252).
 *
 * Answers: "What's the test coverage for this domain?" Aggregates
 * coverage-summary.json across the files the graph says belong to the
 * domain (via chorus:hasCodeFile, same source the /api/chorus/domain/:name/code
 * endpoint already reads).
 *
 * Scope: domain (the one in ?domain=).
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextCoverageDeps {
  sparql: StampSparqlClient;
  /** Returns file paths (as stored in the graph) belonging to the domain. */
  fetchDomainFiles: (domain: string) => Promise<string[]>;
  /** Returns coverage-summary.json contents, or null if missing. */
  readCoverageSummary: () => string | null;
}

interface CoverageCounts {
  total: number;
  covered: number;
  pct: number;
}

interface FileCoverage {
  lines: CoverageCounts;
  statements: CoverageCounts;
  functions: CoverageCounts;
  branches: CoverageCounts;
}

interface CoverageAggregate {
  domain: string;
  file_count: number;
  files_with_coverage: number;
  files_without_coverage: number;
  lines: CoverageCounts;
  statements: CoverageCounts;
  functions: CoverageCounts;
  branches: CoverageCounts;
}

export interface ContextCoverageResponse {
  status: number;
  body: ContextEnvelope<CoverageAggregate> | { error: string };
}

export async function fetchContextCoverage(
  deps: ContextCoverageDeps,
  sourceUrl: string,
  domain?: string,
): Promise<ContextCoverageResponse> {
  if (!domain) {
    return { status: 400, body: { error: 'Missing required parameter: domain' } };
  }
  const raw = deps.readCoverageSummary();
  if (raw === null) {
    return { status: 503, body: { error: 'Coverage summary not available; run tests with --coverage.' } };
  }
  let summary: Record<string, FileCoverage>;
  try {
    summary = JSON.parse(raw) as Record<string, FileCoverage>;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `coverage-summary.json unparseable: ${message}` } };
  }

  const files = await deps.fetchDomainFiles(domain);
  const aggregate = aggregateCoverage(domain, files, summary);

  const header = await stampHeader(deps.sparql, domain);
  return { status: 200, body: buildEnvelope(header, sourceUrl, aggregate) };
}

function aggregateCoverage(
  domain: string,
  files: string[],
  summary: Record<string, FileCoverage>,
): CoverageAggregate {
  const entries = Object.entries(summary).filter(([key]) => key !== 'total');
  let withCoverage = 0;
  const acc: Record<keyof FileCoverage, { total: number; covered: number }> = {
    lines: { total: 0, covered: 0 },
    statements: { total: 0, covered: 0 },
    functions: { total: 0, covered: 0 },
    branches: { total: 0, covered: 0 },
  };

  for (const file of files) {
    const match = entries.find(([key]) => key.endsWith(file) || key === file);
    if (!match) continue;
    withCoverage++;
    const [, fc] = match;
    for (const metric of Object.keys(acc) as Array<keyof FileCoverage>) {
      acc[metric].total += fc[metric].total;
      acc[metric].covered += fc[metric].covered;
    }
  }

  const result: Record<keyof FileCoverage, CoverageCounts> = {
    lines: toCounts(acc.lines),
    statements: toCounts(acc.statements),
    functions: toCounts(acc.functions),
    branches: toCounts(acc.branches),
  };

  return {
    domain,
    file_count: files.length,
    files_with_coverage: withCoverage,
    files_without_coverage: files.length - withCoverage,
    ...result,
  };
}

function toCounts(raw: { total: number; covered: number }): CoverageCounts {
  const pct = raw.total > 0 ? Math.round((raw.covered / raw.total) * 10000) / 100 : 0;
  return { total: raw.total, covered: raw.covered, pct };
}
