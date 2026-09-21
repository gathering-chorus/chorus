/**
 * GET /api/athena/validate — Ontology integrity checker (#2180).
 *
 * Runs a fixed list of SHACL-style checks against the chorus ontology graph
 * in Fuseki. Each check is a SELECT whose non-empty result bindings are
 * treated as violations or warnings depending on configured severity.
 *
 * Dependencies injected explicitly so the handler is testable without a
 * SPARQL endpoint / live HTTP:
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

export interface Check {
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
  // Four rules sat here and all four are DELETED by #4237, 2026-09-21 — the runtime
  // mirror of the four shapes removed from sparql/shapes.ttl:
  //   SubProduct must have parent Product · SubProduct must have SubDomain
  //   SubDomain must have parent          · SubDomain has no instances
  //
  // Every one queried GRAPH <urn:chorus:ontology> for `a chorus:SubProduct` or
  // `a chorus:SubDomain`. SubProduct has had zero rows anywhere since #3603, and the
  // 49 SubDomain rows lived in their own domain graphs, never this one. So all four
  // matched zero nodes in every reachable state: they reported clean without ever
  // being able to report anything else, and the validate run counted them as passes.
  // --- CatalogDoc shape (#2554) — runtime mirror of chorus:CatalogDocShape in shapes.ttl ---
  {
    name: 'CatalogDoc must have catalogHref',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#>
        SELECT ?node WHERE { GRAPH <urn:chorus:instances> {
          ?node a chorus:CatalogDoc .
          FILTER NOT EXISTS { ?node chorus:catalogHref ?h }
        }}`,
  },
  {
    name: 'CatalogDoc product must be in vocab (chorus|gathering|consulting)',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#>
        SELECT ?node WHERE { GRAPH <urn:chorus:instances> {
          ?node a chorus:CatalogDoc ; chorus:product ?p .
          FILTER (?p NOT IN ("chorus", "gathering", "consulting"))
        }}`,
  },
  {
    name: 'CatalogDoc subproduct must be in vocab',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#>
        SELECT ?node WHERE { GRAPH <urn:chorus:instances> {
          ?node a chorus:CatalogDoc ; chorus:subproduct ?sp .
          FILTER (?sp NOT IN ("loom", "werk", "athena", "convergence", "clearing", "quality", "borg"))
        }}`,
  },
  {
    name: 'CatalogDoc role must be in vocab (wren|silas|kade|jeff)',
    severity: 'violation',
    query: `PREFIX chorus: <https://jeffbridwell.com/chorus#>
        SELECT ?node WHERE { GRAPH <urn:chorus:instances> {
          ?node a chorus:CatalogDoc ; chorus:role ?r .
          FILTER (?r NOT IN ("wren", "silas", "kade", "jeff"))
        }}`,
  },
];

export interface AthenaValidateDeps {
  sparql: (query: string) => Promise<SparqlBindingsResult>;
  now?: () => number;
  timestamp?: () => string;
  /** #4237 — the check list, injectable. The warning-severity path used to be
   * covered by a test that leaned on whichever real rule happened to carry
   * severity 'warning'; when this card deleted that rule (it targeted the retired
   * chorus:SubDomain) the test went red without anything being broken. A test of
   * the binding should bring its own rule, not depend on the rule set of the day. */
  checks?: Check[];
}

export async function fetchAthenaValidate(deps: AthenaValidateDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const timestamp = deps.timestamp ?? (() => new Date().toISOString());
  const start = now();

  try {
    const violations: Entry[] = [];
    const warnings: Entry[] = [];
    for (const check of (deps.checks ?? CHECKS)) {
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
