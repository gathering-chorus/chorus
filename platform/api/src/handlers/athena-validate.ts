/**
 * GET /api/athena/validate — Ontology integrity checker (#2180).
 *
 * Runs a fixed list of SHACL-style checks against the chorus ontology graph
 * in Fuseki. Each check is a SELECT whose non-empty result bindings are
 * treated as violations or warnings depending on configured severity.
 *
 * Dependencies injected explicitly so the handler is testable without a
 * SPARQL endpoint:
 *   - sparql: runs a SPARQL SELECT query, returns bindings
 *   - now:      returns current epoch ms (default Date.now)
 *   - timestamp: returns wall-clock string for the response envelope
 *
 * Behavior:
 *   - Any binding from a "violation" check becomes a violation entry.
 *   - Any binding from a "warning" check becomes a warning entry.
 *   - valid = (violations.length === 0); warnings do not flip valid.
 *   - Label used when present; otherwise the node URI is stripped of the
 *     chorus prefix for display.
 *   - If sparql() throws at any point, return 500 + { data: { error }, _meta: { error: true } }.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlNodeBinding {
  node: { value: string };
  label?: { value: string };
}

export interface SparqlBindingsResult {
  results: { bindings: SparqlNodeBinding[] };
}

export interface AthenaValidateDeps {
  sparql: (query: string) => Promise<SparqlBindingsResult>;
  now?: () => number;
  timestamp?: () => string;
}

interface Check {
  name: string;
  severity: 'violation' | 'warning';
  query: string;
}

interface Entry {
  node: string;
  constraint: string;
  severity: 'violation' | 'warning';
  message: string;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

const CHECKS: Check[] = [
  {
    name: 'Product must have Domain',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:Product . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasDomain ?d }
        }}`,
  },
  {
    name: 'Product must have ServiceDesign',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:Product . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasServiceDesign ?sd }
        }}`,
  },
  {
    name: 'SubProduct must have parent Product',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubProduct . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?parent chorus:hasSubProduct ?node }
        }}`,
  },
  {
    name: 'SubProduct must have SubDomain',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubProduct . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:hasDomain ?d }
        }}`,
  },
  {
    name: 'SubDomain must have parent',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubDomain . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?parent chorus:hasDomain ?node }
        }}`,
  },
  {
    name: 'SubDomain has no instances (incomplete)',
    severity: 'warning',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?node ?label WHERE { GRAPH <urn:chorus:ontology> {
          ?node a chorus:SubDomain . OPTIONAL { ?node rdfs:label ?label }
          FILTER NOT EXISTS { ?node chorus:contains ?i }
        }}`,
  },
];

export async function fetchAthenaValidate(deps: AthenaValidateDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const timestamp = deps.timestamp ?? (() => new Date().toISOString());
  const start = now();

  const violations: Entry[] = [];
  const warnings: Entry[] = [];

  try {
    for (const check of CHECKS) {
      const result = await deps.sparql(check.query);
      for (const b of result.results.bindings) {
        const node = b.label?.value ?? b.node.value.replace(CHORUS_PREFIX, '');
        const entry: Entry = {
          node,
          constraint: check.name,
          severity: check.severity,
          message: check.name,
        };
        if (check.severity === 'violation') violations.push(entry);
        else warnings.push(entry);
      }
    }
    return {
      status: 200,
      body: {
        valid: violations.length === 0,
        violations,
        warnings,
        checked: CHECKS.length,
        duration_ms: now() - start,
        timestamp: timestamp(),
      },
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: {
        _meta: { source: 'athena', query_name: 'validate', duration_ms: now() - start, error: true },
        data: { error: message },
      },
    };
  }
}
