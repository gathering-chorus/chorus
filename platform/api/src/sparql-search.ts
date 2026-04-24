// SPARQL text search (extracted from server.ts for #2205 wave 4).
//
// Builds a multi-term CONTAINS-based FILTER against Fuseki, parses the
// bindings JSON into a flat SparqlResult[]. Dep-injected fetch keeps
// tests hermetic — no live Fuseki required.

import type { SparqlResult } from './search-fusion';

export type { SparqlResult };

export interface SparqlSearchDeps {
  fusekiUrl: string;
  fetchFn?: typeof fetch;
  requestTimeoutMs?: number;
}

export type SparqlSearch = (query: string, limit: number) => Promise<SparqlResult[]>;

/**
 * Build the SPARQL query string for the given terms + limit. Returns ''
 * if no terms qualify (length > 2 required). Pure function.
 */
export function buildSparqlQuery(terms: string[], limit: number): string {
  const usable = terms.filter(t => t.length > 2);
  if (usable.length === 0) return '';

  const filters = usable.map((_, i) => `CONTAINS(LCASE(?text), LCASE(?term${i}))`).join(' && ');
  const binds = usable.map((t, i) => `BIND("${t.replace(/"/g, '\\"')}" AS ?term${i})`).join('\n    ');

  return `
    PREFIX jb: <https://jeffbridwell.com/ontology#>
    PREFIX dcterms: <http://purl.org/dc/terms/>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    PREFIX schema: <https://schema.org/>

    SELECT DISTINCT ?s ?type ?domain ?label ?text WHERE {
      GRAPH ?g {
        ?s a ?type .
        { ?s dcterms:title ?label } UNION { ?s rdfs:label ?label } UNION { ?s schema:name ?label }
        OPTIONAL { ?s dcterms:description ?desc }
        BIND(COALESCE(CONCAT(STR(?label), " ", COALESCE(STR(?desc), "")), STR(?label)) AS ?text)
        ${binds}
        FILTER(${filters})
      }
      BIND(REPLACE(STR(?g), "http://localhost:3000/pods/jeff/([^/]+)/.*", "$1") AS ?domain)
    }
    LIMIT ${Math.min(limit, 50)}
  `;
}

/** SPARQL binding row — each variable may or may not be bound. */
export type SparqlBinding = Partial<Record<'s' | 'type' | 'domain' | 'label' | 'text', { value?: string }>>;

/**
 * Parse the SPARQL JSON bindings array into SparqlResult[].
 * Pure function, defensive against missing fields.
 */
export function parseSparqlBindings(bindings: SparqlBinding[]): SparqlResult[] {
  return bindings.map((b) => ({
    uri: b.s?.value || '',
    type: (b.type?.value || '').replace(/.*[#/]/, ''),
    domain: b.domain?.value || '',
    label: b.label?.value || '',
    content: b.text?.value || b.label?.value || '',
    score: 0.5, // baseline for RRF merge
  }));
}

const DEFAULT_REQUEST_TIMEOUT_MS = 10000;

export function createSparqlSearch(deps: SparqlSearchDeps): SparqlSearch {
  const fetchFn = deps.fetchFn ?? fetch;
  const timeoutMs = deps.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;

  return async function sparqlSearch(query: string, limit: number): Promise<SparqlResult[]> {
    const terms = query.split(/\s+/).filter(t => t.length > 2);
    if (terms.length === 0) return [];

    const sparql = buildSparqlQuery(terms, limit);
    if (!sparql) return [];

    try {
      const res = await fetchFn(`${deps.fusekiUrl}?query=${encodeURIComponent(sparql)}`, {
        headers: { Accept: 'application/sparql-results+json' },
        signal: AbortSignal.timeout(timeoutMs),
      });
      if (!res.ok) return [];
      const data = (await res.json()) as { results?: { bindings?: SparqlBinding[] } };
      return parseSparqlBindings(data.results?.bindings || []);
    } catch {
      return [];
    }
  };
}
