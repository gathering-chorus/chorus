/**
 * GET /api/athena/subdomains/:id/pages — Page instances for sub-domain (#2187).
 *
 * Returns { route, path, pageType } per Page + byType histogram. Uses the
 * non-OPTIONAL shape (page must have route + filePath + pageType).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlPageBinding {
  route: { value: string };
  filePath: { value: string };
  pageType: { value: string };
}

export interface SparqlPagesResult {
  results: { bindings: SparqlPageBinding[] };
}

export interface AthenaSubdomainPagesDeps {
  sparql: (query: string) => Promise<SparqlPagesResult>;
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

export async function fetchAthenaSubdomainPages(
  deps: AthenaSubdomainPagesDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?route ?filePath ?pageType WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasPage ?page . ?page a chorus:Page ; chorus:route ?route ; chorus:filePath ?filePath ; chorus:pageType ?pageType . } } ORDER BY ?pageType ?route`;
    const result = await deps.sparql(query);
    const pages = result.results.bindings.map((b) => ({
      route: b.route.value,
      path: b.filePath.value,
      pageType: b.pageType.value,
    }));
    const byType = pages.reduce<Record<string, number>>((acc, p) => {
      acc[p.pageType] = (acc[p.pageType] ?? 0) + 1;
      return acc;
    }, {});
    return {
      status: 200,
      body: envelope('subdomain-pages', { subdomain: id, pages, byType }, now() - start, { count: pages.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-pages', { error: message }, now() - start, { error: true }),
    };
  }
}
