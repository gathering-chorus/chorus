/**
 * GET /api/athena/products — List all chorus:Product instances (#2187).
 *
 * Runs the named 'products' SPARQL SELECT against the chorus ontology graph.
 * Each binding maps to { uri, label }. Label falls back to the URI fragment
 * after '#' when rdfs:label is missing; if the URI has no '#', the full URI
 * is used.
 *
 * Deps injected for unit testability:
 *   - sparql: runs a SPARQL SELECT, returns bindings
 *   - loadQuery: named-query loader (filesystem-backed in prod, string fn in tests)
 *   - now: epoch-ms clock (defaults to Date.now)
 *   - envelope: response shaper (defaults to {_meta: {source, query_name, count, duration_ms[, error]}, data})
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlProductBinding {
  product: { value: string };
  label?: { value: string };
}

export interface SparqlProductsResult {
  results: { bindings: SparqlProductBinding[] };
}

export interface AthenaProductsDeps {
  sparql: (query: string) => Promise<SparqlProductsResult>;
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

export async function fetchAthenaProducts(deps: AthenaProductsDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(deps.loadQuery('products'));
    const products = result.results.bindings.map((b) => ({
      uri: b.product.value,
      label: b.label?.value ?? fallbackLabel(b.product.value),
    }));
    return {
      status: 200,
      body: envelope('products', products, now() - start, { count: products.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('products', { error: message }, now() - start, { error: true }),
    };
  }
}
