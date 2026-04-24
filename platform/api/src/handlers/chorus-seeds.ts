/**
 * GET /api/chorus/seeds — Pull seed entries from Fuseki jb:seeds graph (#1869, extracted #2189).
 *
 * Returns up to 50 seed records from the RDF graph urn:jb:seeds/. Each seed
 * carries slug, content (truncated), link info, and optional routing target.
 *
 * Dependencies injected so the handler is testable without a live Fuseki:
 *   - fetchFn: HTTP fetch replacement
 *   - fusekiUrl: base URL (default from env or localhost:3030)
 */

export type FetchFn = (
  url: string,
  init?: { method?: string; headers?: Record<string, string>; body?: string; signal?: AbortSignal },
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>;

export interface SeedsDeps {
  fetchFn?: FetchFn;
  fusekiUrl?: string;
}

export interface Seed {
  slug?: string;
  content?: string;
  seedUrl?: string;
  linkTitle?: string;
  status: string;
  type?: string;
  source?: string;
  seededAt?: string;
  routedTo?: string;
}

export interface SeedsResult {
  status: number;
  body: { seeds: Seed[]; total: number } | { error: string; status?: number; detail?: string };
}

const SEEDS_QUERY = `
  PREFIX jb: <https://jeffbridwell.com/ontology#>
  SELECT DISTINCT ?slug ?content ?seedUrl ?linkTitle ?seededAt ?routedTo
  WHERE {
    GRAPH <urn:jb:seeds/> {
      ?s jb:slug ?slug .
      OPTIONAL { ?s jb:seedContent ?content }
      OPTIONAL { ?s jb:seedUrl ?seedUrl }
      OPTIONAL { ?s jb:linkTitle ?linkTitle }
      OPTIONAL { ?s jb:seededAt ?seededAt }
      OPTIONAL { ?s jb:routedTo ?routedTo }
    }
  }
  LIMIT 50
`;

export async function fetchSeeds({
  fetchFn = globalThis.fetch as FetchFn,
  fusekiUrl = process.env.FUSEKI_URL || 'http://localhost:3030',
}: SeedsDeps = {}): Promise<SeedsResult> {
  try {
    const url = `${fusekiUrl}/pods/query`;
    const response = await fetchFn(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        Accept: 'application/json',
      },
      body: `query=${encodeURIComponent(SEEDS_QUERY)}`,
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) {
      return { status: 502, body: { error: 'Fuseki query failed', status: response.status } };
    }
    const data = (await response.json()) as {
      results: { bindings: Array<Partial<Record<string, { value: string }>>> };
    };
    const seeds: Seed[] = data.results.bindings.map((b) => ({
      slug: b.slug?.value,
      content: b.content?.value?.substring(0, 200),
      seedUrl: b.seedUrl?.value,
      linkTitle: b.linkTitle?.value,
      status: b.status?.value || 'pending',
      type: b.type?.value,
      source: b.source?.value,
      seededAt: b.seededAt?.value,
      routedTo: b.routedTo?.value,
    }));
    return { status: 200, body: { seeds, total: seeds.length } };
  } catch (err) {
    return {
      status: 500,
      body: {
        error: 'Seeds query failed',
        detail: err instanceof Error ? err.message : String(err),
      },
    };
  }
}
