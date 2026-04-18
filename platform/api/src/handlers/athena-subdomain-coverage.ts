/**
 * GET /api/athena/subdomains/:id/coverage — TestCoverage entities pointing at
 * this sub-domain. Returns (testFile, testType, coversDomain) triples. (#2187)
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlTestCoverageBinding {
  testFile: { value: string };
  testType: { value: string };
}

export interface SparqlTestCoverageResult {
  results: { bindings: SparqlTestCoverageBinding[] };
}

export interface AthenaSubdomainCoverageDeps {
  sparql: (query: string) => Promise<SparqlTestCoverageResult>;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function buildCoverageQuery(sdUri: string): string {
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?testFile ?testType WHERE { GRAPH <urn:chorus:instances> { ?tc a chorus:TestCoverage ; chorus:testFile ?testFile ; chorus:testType ?testType ; chorus:covers <${sdUri}> . } } ORDER BY ?testType ?testFile`;
}

export async function fetchAthenaSubdomainCoverage(
  deps: AthenaSubdomainCoverageDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(buildCoverageQuery(`${CHORUS_PREFIX}${id}`));
    const coverage = result.results.bindings.map((b) => ({
      testFile: b.testFile.value,
      testType: b.testType.value,
      coversDomain: id,
    }));
    return {
      status: 200,
      body: envelope('subdomain-coverage', { subdomain: id, coverage }, now() - start, { count: coverage.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-coverage', { error: message }, now() - start, { error: true }),
    };
  }
}

/**
 * GET /api/athena/subdomains/:id/test-coverage — same TestCoverage query, but
 * shaped as { path, type } with a byType histogram. Two endpoints, same SPARQL;
 * kept separate to preserve response shape parity with the inline code. (#2187)
 */
export async function fetchAthenaSubdomainTestCoverage(
  deps: AthenaSubdomainCoverageDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(buildCoverageQuery(`${CHORUS_PREFIX}${id}`));
    const tests = result.results.bindings.map((b) => ({
      path: b.testFile.value,
      type: b.testType.value,
    }));
    const byType = tests.reduce<Record<string, number>>((acc, t) => {
      acc[t.type] = (acc[t.type] ?? 0) + 1;
      return acc;
    }, {});
    return {
      status: 200,
      body: envelope('subdomain-test-coverage', { subdomain: id, tests, byType }, now() - start, { count: tests.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-test-coverage', { error: message }, now() - start, { error: true }),
    };
  }
}
