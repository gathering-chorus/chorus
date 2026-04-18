/**
 * Subdomain entity list handlers — #2180 (follow-on to #2173 AC4).
 *
 * Extracts GET /api/athena/subdomains/:id/{services, pipeline, logs, gaps}
 * from server.ts. All four share shape:
 *   1. Verify the subdomain exists (SPARQL against urn:chorus:ontology).
 *      Returns 404 if not found.
 *   2. Fetch the entity list (SPARQL against urn:chorus:instances).
 *   3. Shape results into {uri, label, ...props} objects.
 *   4. Return envelope with {count}.
 *
 * Common behavior lives in `fetchSubdomainEntities`; each per-entity
 * exported function supplies its own predicate + field spec.
 */

import type { FetchResult } from './sessions';
import type { SparqlResult, SparqlBinding } from './athena-health';
import type { DomainFacetDeps } from './domain-facets';

interface OptionalField {
  /** SPARQL variable name (without leading `?`). */
  sparqlVar: string;
  /** Output object key. */
  outputKey: string;
}

interface EntitySpec {
  envelopeName: string;
  resultsKey: string;
  hasPredicate: string;
  entityVar: string; // SPARQL variable for the entity URI (e.g. 'svc')
  optionalFields: OptionalField[];
}

function buildListQuery(sdUri: string, spec: EntitySpec): string {
  const optionals = spec.optionalFields
    .map((f) => `OPTIONAL { ?${spec.entityVar} ${f.sparqlVar.includes(':') ? f.sparqlVar : `chorus:${f.sparqlVar}`} ?${f.outputKey} }`)
    .join(' ');
  const selectVars = [spec.entityVar, ...spec.optionalFields.map((f) => f.outputKey)]
    .map((v) => `?${v}`)
    .join(' ');
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ${selectVars} WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> ${spec.hasPredicate} ?${spec.entityVar} . ${optionals} } }`;
}

function shapeEntity(binding: SparqlBinding, spec: EntitySpec): Record<string, string | null> {
  const uri = binding[spec.entityVar]?.value || '';
  const entity: Record<string, string | null> = {
    uri,
    label: binding.label?.value || uri.split('#').pop() || '',
  };
  for (const f of spec.optionalFields) {
    if (f.outputKey === 'label') continue; // already handled
    entity[f.outputKey] = binding[f.outputKey]?.value || null;
  }
  return entity;
}

async function subdomainExists(
  deps: DomainFacetDeps,
  sdUri: string,
): Promise<boolean> {
  const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain } } LIMIT 1`;
  const result = await deps.sparql(query);
  return result.results.bindings.length > 0;
}

/**
 * Shared implementation for GET /api/athena/subdomains/:id/<entityKind>.
 * Returns 404 if the subdomain doesn't exist in the ontology graph.
 */
export async function fetchSubdomainEntities(
  deps: DomainFacetDeps,
  subdomainId: string,
  spec: EntitySpec,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;
    const exists = await subdomainExists(deps, sdUri);
    if (!exists) {
      return {
        status: 404,
        body: deps.envelope(
          spec.envelopeName,
          { error: `Sub-domain '${subdomainId}' not found` },
          now() - start,
          { error: true },
        ),
      };
    }
    const result: SparqlResult = await deps.sparql(buildListQuery(sdUri, spec));
    const entities = result.results.bindings.map((b) => shapeEntity(b, spec));
    return {
      status: 200,
      body: deps.envelope(
        spec.envelopeName,
        { subdomain: subdomainId, [spec.resultsKey]: entities },
        now() - start,
        { count: entities.length },
      ),
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: deps.envelope(spec.envelopeName, { error: message }, now() - start, { error: true }),
    };
  }
}

// --- Services: chorus:hasService → {label, type, host, status, health_endpoint} ---

export const servicesSpec: EntitySpec = {
  envelopeName: 'subdomain-services',
  resultsKey: 'services',
  hasPredicate: 'chorus:hasService',
  entityVar: 'svc',
  optionalFields: [
    { sparqlVar: 'rdfs:label', outputKey: 'label' },
    { sparqlVar: 'chorus:serviceType', outputKey: 'type' },
    { sparqlVar: 'chorus:serviceHost', outputKey: 'host' },
    { sparqlVar: 'chorus:serviceStatus', outputKey: 'status' },
    { sparqlVar: 'chorus:healthEndpoint', outputKey: 'health_endpoint' },
  ],
};

export const fetchSubdomainServicesList = (deps: DomainFacetDeps, id: string) =>
  fetchSubdomainEntities(deps, id, servicesSpec);

// --- Pipelines: chorus:hasPipeline → {label, source, harvester, icd, status, last_run} ---

export const pipelineSpec: EntitySpec = {
  envelopeName: 'subdomain-pipeline',
  resultsKey: 'pipelines',
  hasPredicate: 'chorus:hasPipeline',
  entityVar: 'pipe',
  optionalFields: [
    { sparqlVar: 'rdfs:label', outputKey: 'label' },
    { sparqlVar: 'chorus:pipelineSource', outputKey: 'source' },
    { sparqlVar: 'chorus:pipelineHarvester', outputKey: 'harvester' },
    { sparqlVar: 'chorus:pipelineICD', outputKey: 'icd' },
    { sparqlVar: 'chorus:pipelineStatus', outputKey: 'status' },
    { sparqlVar: 'chorus:pipelineLastRun', outputKey: 'last_run' },
  ],
};

export const fetchSubdomainPipelineList = (deps: DomainFacetDeps, id: string) =>
  fetchSubdomainEntities(deps, id, pipelineSpec);

// --- Logs: chorus:hasLogSource → {label, location, retention, status} ---

export const logsSpec: EntitySpec = {
  envelopeName: 'subdomain-logs',
  resultsKey: 'logs',
  hasPredicate: 'chorus:hasLogSource',
  entityVar: 'log',
  optionalFields: [
    { sparqlVar: 'rdfs:label', outputKey: 'label' },
    { sparqlVar: 'chorus:logSourceLocation', outputKey: 'location' },
    { sparqlVar: 'chorus:logSourceRetention', outputKey: 'retention' },
    { sparqlVar: 'chorus:logSourceStatus', outputKey: 'status' },
  ],
};

export const fetchSubdomainLogsList = (deps: DomainFacetDeps, id: string) =>
  fetchSubdomainEntities(deps, id, logsSpec);

// --- Gaps: chorus:hasGap → {label, type, description, severity} ---

export const gapsSpec: EntitySpec = {
  envelopeName: 'subdomain-gaps',
  resultsKey: 'gaps',
  hasPredicate: 'chorus:hasGap',
  entityVar: 'gap',
  optionalFields: [
    { sparqlVar: 'rdfs:label', outputKey: 'label' },
    { sparqlVar: 'chorus:gapType', outputKey: 'type' },
    { sparqlVar: 'chorus:gapDescription', outputKey: 'description' },
    { sparqlVar: 'chorus:gapSeverity', outputKey: 'severity' },
  ],
};

export const fetchSubdomainGapsList = (deps: DomainFacetDeps, id: string) =>
  fetchSubdomainEntities(deps, id, gapsSpec);
