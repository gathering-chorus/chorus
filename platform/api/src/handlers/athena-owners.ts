/**
 * GET /api/athena/owners — Owners with sub-domain counts (#2187).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlOwnerBinding {
  owner: { value: string };
  label?: { value: string };
  count: { value: string };
}

export interface SparqlOwnersResult {
  results: { bindings: SparqlOwnerBinding[] };
}

export interface AthenaOwnersDeps {
  sparql: (query: string) => Promise<SparqlOwnersResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackLabel(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

export async function fetchAthenaOwners(deps: AthenaOwnersDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(deps.loadQuery('owners'));
    const owners = result.results.bindings.map((b) => ({
      uri: b.owner.value,
      label: b.label?.value ?? fallbackLabel(b.owner.value),
      subdomainCount: parseInt(b.count.value, 10),
    }));
    return {
      status: 200,
      body: envelope('owners', owners, now() - start, { count: owners.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('owners', { error: message }, now() - start, { error: true }),
    };
  }
}
