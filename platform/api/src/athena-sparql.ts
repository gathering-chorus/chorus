// Athena SPARQL client + envelope builder + query loader (extracted from
// server.ts for #2205 wave 8).
//
// Three factory-style exports so production keeps a single instance and
// tests can inject fakes for fetch / fs.

export interface AthenaSparqlClientDeps {
  sparqlUrl: string;
  updateUrl: string;
  fetchFn?: typeof fetch;
  /** #3566 LOCK — when set, writes carry HTTP Basic auth. Reads stay open. */
  auth?: { user: string; password: string };
}

/** Raw SPARQL response — structure varies by query; downstream handlers cast
 *  to their expected bindings shape. any retained here because the downstream
 *  SparqlResult/Sparql/SparqlCodeResult types have incompatible required
 *  bindings shapes; a narrower response would force cascading renames across
 *  5+ files. #2463 scope: this gap documented. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SparqlResponse = any;

export interface AthenaSparqlClient {
  query: (query: string) => Promise<SparqlResponse>;
  update: (update: string) => Promise<void>;
}

export function createAthenaSparqlClient(deps: AthenaSparqlClientDeps): AthenaSparqlClient {
  const fetchFn = deps.fetchFn ?? fetch;
  return {
    async query(query: string): Promise<SparqlResponse> {
      const res = await fetchFn(deps.sparqlUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/sparql-query',
          Accept: 'application/sparql-results+json',
        },
        body: query,
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`Fuseki ${res.status}: ${text.slice(0, 200)}`);
      }
      return res.json();
    },
    async update(update: string): Promise<void> {
      const headers: Record<string, string> = { 'Content-Type': 'application/sparql-update' };
      if (deps.auth) headers['Authorization'] = 'Basic ' + Buffer.from(`${deps.auth.user}:${deps.auth.password}`).toString('base64');
      const res = await fetchFn(deps.updateUrl, {
        method: 'POST',
        headers,
        body: update,
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`Fuseki update ${res.status}: ${text.slice(0, 200)}`);
      }
    },
  };
}

export interface EnvelopeBuilderDeps {
  graph: string;
  now: () => string;
}

export type EnvelopeBuilder = (
  queryName: string,
  data: unknown,
  durationMs: number,
  extra?: Record<string, unknown>,
) => { _meta: Record<string, unknown>; data: unknown };

export function createEnvelopeBuilder(deps: EnvelopeBuilderDeps): EnvelopeBuilder {
  return (queryName, data, durationMs, extra = {}) => ({
    _meta: {
      source: 'athena',
      query_name: queryName,
      graph: deps.graph,
      duration_ms: durationMs,
      cached: false,
      timestamp: deps.now(),
      ...extra,
    },
    data,
  });
}

export interface SparqlLoaderDeps {
  fs: { readFileSync: (path: string, enc: BufferEncoding) => string };
  sparqlDir: string;
}

export function createSparqlLoader(deps: SparqlLoaderDeps): (name: string) => string {
  return (name: string) => String(deps.fs.readFileSync(`${deps.sparqlDir}/${name}.sparql`, 'utf-8')).trim();
}
