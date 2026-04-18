/**
 * GET /api/athena/subdomains — List sub-domains with owner + step (#2187).
 *
 * Supports optional query filters:
 *   - owner: case-insensitive match on rdfs:label of owner
 *   - step:  case-insensitive match on rdfs:label of step
 *
 * Filters are injected into the SPARQL query text as FILTER(LCASE(...))
 * clauses before the ORDER BY. Matches the pre-extraction behavior
 * verbatim.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlSubdomainBinding {
  sd: { value: string };
  label?: { value: string };
  ownerLabel?: { value: string };
  stepLabel?: { value: string };
}

export interface SparqlSubdomainsResult {
  results: { bindings: SparqlSubdomainBinding[] };
}

export interface SubdomainsQuery {
  owner?: string;
  step?: string;
}

export interface AthenaSubdomainsDeps {
  sparql: (query: string) => Promise<SparqlSubdomainsResult>;
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

function fallbackId(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

export async function fetchAthenaSubdomains(
  deps: AthenaSubdomainsDeps,
  query: SubdomainsQuery,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    let sparqlText = deps.loadQuery('subdomains');
    const filters: string[] = [];
    if (query.owner) {
      filters.push(`FILTER(LCASE(STR(?ownerLabel)) = "${String(query.owner).toLowerCase()}")`);
    }
    if (query.step) {
      filters.push(`FILTER(LCASE(STR(?stepLabel)) = "${String(query.step).toLowerCase()}")`);
    }
    if (filters.length) {
      sparqlText = sparqlText.replace('} ORDER BY', `${filters.join('\n  ')}\n} ORDER BY`);
    }
    const result = await deps.sparql(sparqlText);
    const subdomains = result.results.bindings.map((b) => ({
      uri: b.sd.value,
      id: fallbackId(b.sd.value),
      label: b.label?.value ?? fallbackId(b.sd.value),
      owner: b.ownerLabel?.value ?? null,
      step: b.stepLabel?.value ?? null,
    }));
    return {
      status: 200,
      body: envelope('subdomains', subdomains, now() - start, {
        count: subdomains.length,
        filters: { owner: query.owner ?? null, step: query.step ?? null },
      }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomains', { error: message }, now() - start, { error: true }),
    };
  }
}
