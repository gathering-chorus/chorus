/**
 * GET /api/athena/subdomains/:id/services — Endpoint instances for sub-domain (#2187).
 *
 * Returns { method, path, handler } per Endpoint + byMethod histogram.
 * Named "services" in the route but about Endpoints, preserving the
 * response field name `endpoints`.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlEndpointBinding {
  method: { value: string };
  routePath: { value: string };
  filePath: { value: string };
}

export interface SparqlEndpointsResult {
  results: { bindings: SparqlEndpointBinding[] };
}

export interface AthenaSubdomainEndpointsDeps {
  sparql: (query: string) => Promise<SparqlEndpointsResult>;
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

export async function fetchAthenaSubdomainEndpoints(
  deps: AthenaSubdomainEndpointsDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?method ?routePath ?filePath WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasEndpoint ?ep . ?ep a chorus:Endpoint ; chorus:httpMethod ?method ; chorus:routePath ?routePath ; chorus:filePath ?filePath . } } ORDER BY ?method ?routePath`;
    const result = await deps.sparql(query);
    const endpoints = result.results.bindings.map((b) => ({
      method: b.method.value,
      path: b.routePath.value,
      handler: b.filePath.value,
    }));
    const byMethod = endpoints.reduce<Record<string, number>>((acc, e) => {
      acc[e.method] = (acc[e.method] ?? 0) + 1;
      return acc;
    }, {});
    return {
      status: 200,
      body: envelope('subdomain-services', { subdomain: id, endpoints, byMethod }, now() - start, { count: endpoints.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-services', { error: message }, now() - start, { error: true }),
    };
  }
}
