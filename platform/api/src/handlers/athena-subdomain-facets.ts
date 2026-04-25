/**
 * Generic facet fetcher for sub-domain OPTIONAL-field endpoints (#2187).
 *
 * Six routes share the same shape:
 *   - 404 if sub-domain doesn't exist in the ontology graph
 *   - SELECT ?item + OPTIONAL ?field1 ?field2 ... from the instances graph
 *   - map bindings to { uri, label, ...fields } with undefined for missing fields
 *     (label falls back to URI fragment)
 *
 * Routes: actors, scenarios, contract, integrations, persistence, prior-art.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlFacetBinding {
  [key: string]: { value: string } | undefined;
}

export interface SparqlFacetResult {
  results: { bindings: SparqlFacetBinding[] };
}

export interface AthenaFacetDeps {
  sparql: (query: string) => Promise<SparqlFacetResult>;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

export interface FacetSpec {
  /** envelope query_name (also used in response data key) */
  queryName: string;
  /** JSON key for the results array in the response body */
  collectionKey: string;
  /** SPARQL ?var name for the item URI (e.g., 'actor', 'scenario', 'contract') */
  itemVar: string;
  /** chorus:hasX predicate (e.g., 'hasActor') */
  predicate: string;
  /** OPTIONAL field specs. `transform` optionally coerces the string value
   *  (e.g., parseInt for numeric fields). Missing fields stay as null. */
  fields: Array<{
    sparqlVar: string;
    outputKey: string;
    rdfProp: string;
    transform?: (value: string) => string | number | boolean | null;
  }>;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackId(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

/** Build the primary SELECT query for a facet. */
function buildFacetQuery(sdUri: string, spec: FacetSpec): string {
  const varList = [spec.itemVar, 'label', ...spec.fields.map((f) => f.sparqlVar)].map((v) => `?${v}`).join(' ');
  const optionals = [
    `OPTIONAL { ?${spec.itemVar} rdfs:label ?label }`,
    ...spec.fields.map((f) => `OPTIONAL { ?${spec.itemVar} ${f.rdfProp} ?${f.sparqlVar} }`),
  ].join(' ');
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ${varList} WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:${spec.predicate} ?${spec.itemVar} . ${optionals} } }`;
}

const EXISTS_QUERY = (sdUri: string) =>
  `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`;

export async function fetchAthenaFacet(
  deps: AthenaFacetDeps,
  id: string,
  spec: FacetSpec,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const exists = await deps.sparql(EXISTS_QUERY(sdUri));
    if (exists.results.bindings.length === 0) {
      return {
        status: 404,
        body: envelope(
          spec.queryName,
          { error: `Sub-domain '${id}' not found` },
          now() - start,
          { error: true },
        ),
      };
    }
    const result = await deps.sparql(buildFacetQuery(sdUri, spec));
    // Missing optional fields are returned as null (not undefined), so the
    // value is JSON-visible to clients as `"key": null` rather than stripped.
    // Matches the pre-extraction shape of scenarios/contract/integrations/
    // persistence/prior-art. Actors shifts from undefined→null (minor).
    const items = result.results.bindings.map((b) => {
      const itemUri = b[spec.itemVar]!.value;
      const base: Record<string, string | number | boolean | null> = {
        uri: itemUri,
        label: b.label?.value ?? fallbackId(itemUri),
      };
      for (const f of spec.fields) {
        const raw = b[f.sparqlVar]?.value;
        base[f.outputKey] = raw == null ? null : (f.transform ? f.transform(raw) : raw);
      }
      return base;
    });
    return {
      status: 200,
      body: envelope(
        spec.queryName,
        { subdomain: id, [spec.collectionKey]: items },
        now() - start,
        { count: items.length },
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope(spec.queryName, { error: message }, now() - start, { error: true }),
    };
  }
}

// --- Specific facet functions ---

export const fetchAthenaSubdomainActors = (deps: AthenaFacetDeps, id: string) =>
  fetchAthenaFacet(deps, id, {
    queryName: 'subdomain-actors',
    collectionKey: 'actors',
    itemVar: 'actor',
    predicate: 'hasActor',
    fields: [
      { sparqlVar: 'role', outputKey: 'role', rdfProp: 'chorus:actorRole' },
      { sparqlVar: 'action', outputKey: 'action', rdfProp: 'chorus:actorAction' },
    ],
  });

export const fetchAthenaSubdomainScenarios = (deps: AthenaFacetDeps, id: string) =>
  fetchAthenaFacet(deps, id, {
    queryName: 'subdomain-scenarios',
    collectionKey: 'scenarios',
    itemVar: 'scenario',
    predicate: 'hasScenario',
    fields: [
      { sparqlVar: 'given', outputKey: 'given', rdfProp: 'chorus:scenarioGiven' },
      { sparqlVar: 'when', outputKey: 'when', rdfProp: 'chorus:scenarioWhen' },
      { sparqlVar: 'then', outputKey: 'then', rdfProp: 'chorus:scenarioThen' },
      { sparqlVar: 'notes', outputKey: 'notes', rdfProp: 'chorus:scenarioNotes' },
    ],
  });

export const fetchAthenaSubdomainContract = (deps: AthenaFacetDeps, id: string) =>
  fetchAthenaFacet(deps, id, {
    queryName: 'subdomain-contract',
    collectionKey: 'endpoints',
    itemVar: 'contract',
    predicate: 'hasContract',
    fields: [
      { sparqlVar: 'endpoint', outputKey: 'path', rdfProp: 'chorus:endpoint' },
      { sparqlVar: 'method', outputKey: 'method', rdfProp: 'chorus:httpMethod' },
      { sparqlVar: 'description', outputKey: 'description', rdfProp: 'chorus:contractDescription' },
    ],
  });

export const fetchAthenaSubdomainIntegrations = (deps: AthenaFacetDeps, id: string) =>
  fetchAthenaFacet(deps, id, {
    queryName: 'subdomain-integrations',
    collectionKey: 'integrations',
    itemVar: 'integration',
    predicate: 'hasIntegration',
    fields: [
      { sparqlVar: 'source', outputKey: 'source', rdfProp: 'chorus:integrationSource' },
      { sparqlVar: 'path', outputKey: 'path', rdfProp: 'chorus:integrationPath' },
      { sparqlVar: 'status', outputKey: 'status', rdfProp: 'chorus:integrationStatus' },
    ],
  });

export const fetchAthenaSubdomainPersistence = (deps: AthenaFacetDeps, id: string) =>
  fetchAthenaFacet(deps, id, {
    queryName: 'subdomain-persistence',
    collectionKey: 'stores',
    itemVar: 'store',
    predicate: 'hasPersistence',
    fields: [
      { sparqlVar: 'type', outputKey: 'type', rdfProp: 'chorus:storeType' },
      { sparqlVar: 'namespace', outputKey: 'namespace', rdfProp: 'chorus:storeNamespace' },
      { sparqlVar: 'records', outputKey: 'records', rdfProp: 'chorus:storeRecordCount', transform: (v) => parseInt(v, 10) },
      { sparqlVar: 'status', outputKey: 'status', rdfProp: 'chorus:storeStatus' },
    ],
  });

// #2485 — prior-art has TWO sources:
//   (a) hand-authored chorus:hasPriorArt items
//   (b) ADRs (chorus:Decision with decisionType="ADR") that have
//       chorus:hasDomain pointing at this subdomain
// UNION query returns both; each item carries `source: 'authored' | 'adr'`
// so the page can differentiate on render.
export async function fetchAthenaSubdomainPriorArt(
  deps: AthenaFacetDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const exists = await deps.sparql(EXISTS_QUERY(sdUri));
    if (exists.results.bindings.length === 0) {
      return {
        status: 404,
        body: envelope('subdomain-prior-art', { error: `Sub-domain '${id}' not found` }, now() - start, { error: true }),
      };
    }
    const handAuthored = await deps.sparql(
      `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?item ?label ?path ?description WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasPriorArt ?item . OPTIONAL { ?item rdfs:label ?label } OPTIONAL { ?item chorus:filePath ?path } OPTIONAL { ?item rdfs:comment ?description } } }`,
    );
    const adrDerived = await deps.sparql(
      `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?item ?label ?description WHERE { GRAPH <urn:chorus:instances> { ?item a chorus:Decision ; chorus:decisionType "ADR" ; chorus:hasDomain <${sdUri}> . OPTIONAL { ?item rdfs:label ?label } OPTIONAL { ?item rdfs:comment ?description } } }`,
    );
    const items = [
      ...handAuthored.results.bindings.map((b) => {
        const itemUri = b.item!.value;
        return {
          uri: itemUri,
          label: b.label?.value ?? fallbackId(itemUri),
          path: b.path?.value ?? null,
          description: b.description?.value ?? null,
          source: 'authored' as const,
        };
      }),
      ...adrDerived.results.bindings.map((b) => {
        const itemUri = b.item!.value;
        // URI shape: urn:chorus:decision:adr_NNN (or chorus#adr-NNN). Extract
        // the adr_NNN tail, normalize to the canonical filename ADR-NNN-*.md.
        const tail = itemUri.split(/[:#]/).pop() ?? itemUri;
        const adrLabel = b.label?.value ?? tail.toUpperCase().replace(/_/g, '-');
        return {
          uri: itemUri,
          label: adrLabel,
          // Best-effort path; canonical filename glob is ADR-NNN-*.md.
          path: `roles/silas/adr/${adrLabel}*.md`,
          description: b.description?.value ?? null,
          source: 'adr' as const,
        };
      }),
    ];
    return {
      status: 200,
      body: envelope('subdomain-prior-art', { subdomain: id, items }, now() - start, { count: items.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-prior-art', { error: message }, now() - start, { error: true }),
    };
  }
}
