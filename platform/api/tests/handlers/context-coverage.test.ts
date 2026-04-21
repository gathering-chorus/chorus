/**
 * context-coverage handler tests (#2252).
 */

import {
  fetchContextCoverage,
  type ContextCoverageDeps,
} from '../../src/handlers/context-coverage';

function stubSparql(): ContextCoverageDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

const COVERAGE_SUMMARY = JSON.stringify({
  total: {
    lines: { total: 100, covered: 80, pct: 80 },
    statements: { total: 120, covered: 90, pct: 75 },
    functions: { total: 20, covered: 15, pct: 75 },
    branches: { total: 40, covered: 28, pct: 70 },
  },
  '/abs/path/chorus/platform/api/src/chorus-domain.ts': {
    lines: { total: 50, covered: 45, pct: 90 },
    functions: { total: 10, covered: 9, pct: 90 },
    statements: { total: 60, covered: 54, pct: 90 },
    branches: { total: 20, covered: 16, pct: 80 },
  },
  '/abs/path/chorus/platform/api/src/chorus-freshness.ts': {
    lines: { total: 50, covered: 35, pct: 70 },
    functions: { total: 10, covered: 6, pct: 60 },
    statements: { total: 60, covered: 36, pct: 60 },
    branches: { total: 20, covered: 12, pct: 60 },
  },
});

function depsFor(files: string[], summary = COVERAGE_SUMMARY): ContextCoverageDeps {
  return {
    sparql: stubSparql(),
    fetchDomainFiles: async () => files,
    readCoverageSummary: () => summary,
  };
}

describe('fetchContextCoverage', () => {
  it('returns aggregate coverage for a domain', async () => {
    const r = await fetchContextCoverage(
      depsFor(['chorus/platform/api/src/chorus-domain.ts', 'chorus/platform/api/src/chorus-freshness.ts']),
      '/api/chorus/context/coverage?domain=chorus',
      'chorus',
    );
    expect(r.status).toBe(200);
    const body = r.body as {
      data: {
        domain: string; file_count: number;
        lines: { total: number; covered: number; pct: number };
      };
    };
    expect(body.data.domain).toBe('chorus');
    expect(body.data.file_count).toBe(2);
    expect(body.data.lines.total).toBe(100);
    expect(body.data.lines.covered).toBe(80);
    expect(body.data.lines.pct).toBe(80);
  });

  it('missing ?domain param → 400', async () => {
    const r = await fetchContextCoverage(depsFor([]), '/api/chorus/context/coverage');
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toMatch(/domain/);
  });

  it('domain with no files → file_count 0, zero totals', async () => {
    const r = await fetchContextCoverage(
      depsFor([]),
      '/api/chorus/context/coverage?domain=unknown',
      'unknown',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { file_count: number; lines: { total: number } } };
    expect(body.data.file_count).toBe(0);
    expect(body.data.lines.total).toBe(0);
  });

  it('files present in graph but absent from coverage-summary are counted zero, not crash', async () => {
    const r = await fetchContextCoverage(
      depsFor(['chorus/platform/api/src/nonexistent.ts']),
      '/api/chorus/context/coverage?domain=chorus',
      'chorus',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { file_count: number; files_without_coverage: number } };
    expect(body.data.file_count).toBe(1);
    expect(body.data.files_without_coverage).toBe(1);
  });

  it('503 when coverage-summary missing', async () => {
    const r = await fetchContextCoverage(
      {
        sparql: stubSparql(),
        fetchDomainFiles: async () => [],
        readCoverageSummary: () => null,
      },
      '/api/chorus/context/coverage?domain=chorus',
      'chorus',
    );
    expect(r.status).toBe(503);
  });

  it('envelope has source + ISO timestamp + domain set to requested domain', async () => {
    const r = await fetchContextCoverage(
      depsFor(['chorus/platform/api/src/chorus-domain.ts']),
      '/api/chorus/context/coverage?domain=chorus',
      'chorus',
    );
    const body = r.body as { source: string; timestamp: string; domain?: string };
    expect(body.source).toBe('/api/chorus/context/coverage?domain=chorus');
    expect(body.domain).toBe('chorus');
    expect(body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T.*Z$/);
  });
});
