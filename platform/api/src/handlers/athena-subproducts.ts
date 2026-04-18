/**
 * GET /api/athena/subproducts — List sub-products with owner + counts (#2187).
 *
 * Runs the named 'subproducts' SPARQL SELECT. Each binding maps to
 * { uri, label, owner, domainCount, consumesCount }.
 *
 * Missing fields have stable defaults:
 *   - label → URI fragment after '#' (or full URI if no '#')
 *   - owner → null
 *   - domainCount / consumesCount → 0
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlSubproductBinding {
  sp: { value: string };
  label?: { value: string };
  ownerLabel?: { value: string };
  domainCount?: { value: string };
  consumesCount?: { value: string };
}

export interface SparqlSubproductsResult {
  results: { bindings: SparqlSubproductBinding[] };
}

export interface AthenaSubproductsDeps {
  sparql: (query: string) => Promise<SparqlSubproductsResult>;
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

export async function fetchAthenaSubproducts(deps: AthenaSubproductsDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(deps.loadQuery('subproducts'));
    const subproducts = result.results.bindings.map((b) => ({
      uri: b.sp.value,
      label: b.label?.value ?? fallbackLabel(b.sp.value),
      owner: b.ownerLabel?.value ?? null,
      domainCount: parseInt(b.domainCount?.value ?? '0', 10),
      consumesCount: parseInt(b.consumesCount?.value ?? '0', 10),
    }));
    return {
      status: 200,
      body: envelope('subproducts', subproducts, now() - start, { count: subproducts.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subproducts', { error: message }, now() - start, { error: true }),
    };
  }
}
