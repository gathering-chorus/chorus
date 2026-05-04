/**
 * GET /api/athena/health — Athena ontology liveness (#2173 AC4).
 *
 * Second handler extraction. Unlike codebase-topology (HTTP proxy), this one
 * depends on a SPARQL client. Proves the injected-client pattern for the
 * 40+ Athena-backed handlers that will follow.
 *
 * Dependencies passed in explicitly:
 *   - sparql: runs a SPARQL query, returns result rows
 *   - loadQuery: returns a named SPARQL query text (defaults are
 *     filesystem-backed in production; tests pass a string-returning fn)
 *
 * Behavior:
 *   - Success path: parse `count` from `result.results.bindings[0].count.value`,
 *     return 200 + envelope with tripleCount
 *   - Failure path: SPARQL throws → 503 + envelope with error flag
 */

import type { FetchResult } from './codebase-topology';

export interface SparqlBinding {
  [key: string]: { value: string } | undefined;
}

export interface SparqlResult {
  results: { bindings: SparqlBinding[] };
}

const DEFAULT_ENDPOINT = 'http://localhost:3030/pods/sparql';

const DEFAULT_QUERIES = [
  { name: 'health', path: '/api/athena/health', description: 'Ontology health — triple count, endpoint status' },
  { name: 'products', path: '/api/athena/products', description: 'List all products' },
  { name: 'subproducts', path: '/api/athena/subproducts', description: 'List sub-products with owner, domain count, consumes count' },
  { name: 'subdomains', path: '/api/athena/subdomains', description: 'List sub-domains with owner, step. Filter: ?owner, ?step' },
  { name: 'blast-radius', path: '/api/athena/subdomains/:id/blast-radius', description: 'Which sub-products consume a given sub-domain' },
  { name: 'subdomain-principles', path: '/api/athena/subdomains/:id/principles', description: 'Principles inside a SubDomain (loom-principles)' },
  { name: 'subdomain-decisions', path: '/api/athena/subdomains/:id/decisions', description: 'Decisions (DECs + ADRs) inside a SubDomain (loom-decisions, #2716)' },
  { name: 'steps', path: '/api/athena/steps', description: 'Value stream steps with sub-domains at each step' },
  { name: 'owners', path: '/api/athena/owners', description: 'Owners with sub-domain counts' },
  { name: 'machines', path: '/api/athena/machines', description: 'Machines with running services' },
];

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

export interface AthenaHealthDeps {
  sparql: (query: string) => Promise<SparqlResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

export async function fetchAthenaHealth(deps: AthenaHealthDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(deps.loadQuery('health'));
    const countStr = result.results.bindings[0]?.count?.value || '0';
    const count = parseInt(countStr, 10);
    const body = envelope(
      'health',
      { status: 'ok', tripleCount: count, endpoint: DEFAULT_ENDPOINT, queries: DEFAULT_QUERIES },
      now() - start,
    );
    return { status: 200, body };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    const body = envelope(
      'health',
      { status: 'error', message, queries: DEFAULT_QUERIES },
      now() - start,
      { error: true },
    );
    return { status: 503, body };
  }
}
