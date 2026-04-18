/**
 * GET /api/chorus/harvest — Harvest pipeline status (#1485, extracted #2189).
 *
 * Queries Fuseki for triple counts per named graph, then aggregates graphs
 * by domain prefix (e.g. "music-albums.ttl" and "music-plays.ttl" both roll
 * into domain "music"). Returned sorted by triple count, descending.
 *
 * Dependencies injected so tests run without a live Fuseki.
 */

export type FetchFn = (
  url: string,
  init?: { headers?: Record<string, string>; signal?: AbortSignal },
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>;

export interface HarvestDeps {
  fetchFn?: FetchFn;
  fusekiUrl?: string;
}

export interface HarvestResult {
  status: number;
  body:
    | { total_graphs: number; total_triples: number; domains: Array<{ name: string; graphs: number; triples: number }> }
    | { error: string; detail?: string };
}

const HARVEST_QUERY = `
  SELECT ?g (COUNT(*) AS ?count) WHERE {
    GRAPH ?g { ?s ?p ?o }
  } GROUP BY ?g ORDER BY DESC(?count)
`;

export async function fetchHarvest({
  fetchFn = globalThis.fetch as FetchFn,
  fusekiUrl = process.env.FUSEKI_URL || 'http://localhost:3030',
}: HarvestDeps = {}): Promise<HarvestResult> {
  try {
    const response = await fetchFn(`${fusekiUrl}?query=${encodeURIComponent(HARVEST_QUERY)}`, {
      headers: { Accept: 'application/sparql-results+json' },
      signal: AbortSignal.timeout(30000),
    });
    if (!response.ok) {
      return { status: 500, body: { error: 'Fuseki query failed', detail: `Fuseki: ${response.status}` } };
    }
    const data = (await response.json()) as {
      results: { bindings: Array<{ g?: { value: string }; count?: { value: string } }> };
    };

    const graphs: { name: string; triples: number }[] = [];
    let totalTriples = 0;
    for (const b of data.results.bindings) {
      const name = (b.g?.value || '').split('/').pop() || '';
      const count = parseInt(b.count?.value || '0', 10);
      graphs.push({ name, triples: count });
      totalTriples += count;
    }

    const domains: Record<string, { graphs: number; triples: number }> = {};
    for (const g of graphs) {
      const domain = g.name.replace(/[-_].*$/, '').replace(/\.ttl$/, '').toLowerCase();
      if (!domains[domain]) domains[domain] = { graphs: 0, triples: 0 };
      domains[domain].graphs++;
      domains[domain].triples += g.triples;
    }

    return {
      status: 200,
      body: {
        total_graphs: graphs.length,
        total_triples: totalTriples,
        domains: Object.entries(domains)
          .sort((a, b) => b[1].triples - a[1].triples)
          .map(([name, d]) => ({ name, ...d })),
      },
    };
  } catch (err) {
    return {
      status: 500,
      body: { error: 'Fuseki query failed', detail: String(err) },
    };
  }
}
