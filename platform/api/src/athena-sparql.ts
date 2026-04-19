// Athena SPARQL client + envelope builder + query loader (extracted from
// server.ts for #2205 wave 8).
//
// Three factory-style exports so production keeps a single instance and
// tests can inject fakes for fetch / fs.

export interface AthenaSparqlClientDeps {
  sparqlUrl: string;
  updateUrl: string;
  fetchFn?: typeof fetch;
}

export interface AthenaSparqlClient {
  query: (query: string) => Promise<any>;
  update: (update: string) => Promise<void>;
}

export function createAthenaSparqlClient(deps: AthenaSparqlClientDeps): AthenaSparqlClient {
  const fetchFn = deps.fetchFn ?? fetch;
  return {
    async query(query: string): Promise<any> {
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
      const res = await fetchFn(deps.updateUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/sparql-update' },
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
  data: any,
  durationMs: number,
  extra?: Record<string, any>,
) => { _meta: Record<string, any>; data: any };

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
  fs: { readFileSync: (...args: any[]) => any };
  sparqlDir: string;
}

export function createSparqlLoader(deps: SparqlLoaderDeps): (name: string) => string {
  return (name: string) => String(deps.fs.readFileSync(`${deps.sparqlDir}/${name}.sparql`, 'utf-8')).trim();
}
