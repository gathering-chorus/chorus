// ICD (Interface Control Document) SPARQL client + domain resolver
// (extracted from server.ts for #2205 wave 9).
//
// Differs from athena-sparql.ts: query uses GET with ?query= instead of
// POST body. Error messages use different prefixes (`SPARQL query failed`
// vs `Fuseki`). Dedicated resolver turns a domainId into a URI via two-
// phase lookup (id then name).

export interface IcdSparqlClientDeps {
  queryUrl: string;
  updateUrl: string;
  fetchFn?: typeof fetch;
}

export interface IcdSparqlClient {
  query: (query: string) => Promise<any>;
  update: (update: string) => Promise<void>;
}

export function createIcdSparqlClient(deps: IcdSparqlClientDeps): IcdSparqlClient {
  const fetchFn = deps.fetchFn ?? fetch;
  return {
    async query(query: string): Promise<any> {
      const resp = await fetchFn(`${deps.queryUrl}?query=${encodeURIComponent(query)}`, {
        headers: { Accept: 'application/sparql-results+json' },
      });
      if (!resp.ok) throw new Error(`SPARQL query failed: ${resp.status}`);
      return resp.json();
    },
    async update(update: string): Promise<void> {
      const resp = await fetchFn(deps.updateUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/sparql-update' },
        body: update,
      });
      if (!resp.ok) {
        const body = await resp.text();
        throw new Error(`SPARQL update failed: ${resp.status} — ${body}`);
      }
    },
  };
}

export interface IcdDomainResolverDeps {
  client: Pick<IcdSparqlClient, 'query'>;
  pfx: string;
  graph: string;
}

/**
 * Two-phase domain resolver:
 * 1. Match on `icd:domainId` literal "domain-<domainId>".
 * 2. Fall back to case-insensitive `icd:domainName` match.
 * Returns the URI of the matching Domain or null.
 */
export function createIcdDomainResolver(deps: IcdDomainResolverDeps): (domainId: string) => Promise<string | null> {
  return async (domainId: string): Promise<string | null> => {
    const byId = await deps.client.query(
      `${deps.pfx} SELECT ?d WHERE { GRAPH <${deps.graph}> { ?d a icd:Domain ; icd:domainId ?did . FILTER(?did = "domain-${domainId}") } } LIMIT 1`,
    );
    if (byId.results.bindings.length > 0) return byId.results.bindings[0].d.value;
    const byName = await deps.client.query(
      `${deps.pfx} SELECT ?d WHERE { GRAPH <${deps.graph}> { ?d a icd:Domain ; icd:domainName ?name . FILTER(LCASE(?name) = "${domainId.toLowerCase()}") } } LIMIT 1`,
    );
    return byName.results.bindings.length > 0 ? byName.results.bindings[0].d.value : null;
  };
}
