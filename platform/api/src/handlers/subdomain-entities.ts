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

// --- POST create: shared write-path ---
//
// All four entity POSTs share: validate `label`, mint URI from label slug,
// INSERT {entity type, label, hasX edge from subdomain, optional props}
// into urn:chorus:instances. Spec maps request-body fields to chorus: predicates.

export interface PropertyDescriptor {
  /** Full predicate QName. Short form (no colon) → 'chorus:<value>'. */
  predicate: string;
  /** 'literal' (default) → "value"; 'uri' → <chorus#value>. */
  kind?: 'literal' | 'uri';
}

export interface CreateEntitySpec {
  envelopeName: string;          // 'subdomain-service-create'
  uriSegment: string;            // 'service' — used in URI slug and type name
  typeClass: string;             // 'chorus:Service'
  hasPredicate: string;          // 'chorus:hasService'
  /**
   * Map of request-body-field → predicate.
   * String form: literal triple on chorus:<value>. Descriptor form: full
   * predicate + kind (literal vs uri ref).
   */
  propertyMap: Record<string, string | PropertyDescriptor>;
  /** Optional normalization on the request body before triples are built. */
  normalize?: (body: Record<string, unknown>) => Record<string, unknown>;
}

function resolveDescriptor(d: string | PropertyDescriptor): PropertyDescriptor {
  if (typeof d === 'string') {
    return { predicate: d.includes(':') ? d : `chorus:${d}`, kind: 'literal' };
  }
  return { kind: 'literal', ...d };
}

function serializeTriple(entityUri: string, desc: PropertyDescriptor, value: string): string {
  if (desc.kind === 'uri') {
    return `<${entityUri}> ${desc.predicate} <https://jeffbridwell.com/chorus#${value}> .`;
  }
  return `<${entityUri}> ${desc.predicate} "${escapeLiteral(value)}" .`;
}

export interface WriteDeps extends DomainFacetDeps {
  sparqlUpdate: (update: string) => Promise<void>;
}

function escapeLiteral(s: string): string {
  return s.replace(/"/g, '\\"');
}

function slugify(s: string): string {
  return s.toLowerCase().replace(/\s+/g, '-');
}

export async function createSubdomainEntity(
  deps: WriteDeps,
  subdomainId: string,
  body: Record<string, unknown> | null | undefined,
  spec: CreateEntitySpec,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const rawBody = (body || {}) as Record<string, unknown>;
    const b = (spec.normalize ? spec.normalize(rawBody) : rawBody) as Record<string, unknown>;
    const label = b.label;
    if (!label || typeof label !== 'string') {
      return {
        status: 400,
        body: deps.envelope(spec.envelopeName, { error: 'Missing required field: label' }, now() - start, { error: true }),
      };
    }
    const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;
    const entityId = `${subdomainId}-${spec.uriSegment}-${slugify(label)}`;
    const entityUri = `https://jeffbridwell.com/chorus#${entityId}`;

    const propTriples = Object.entries(spec.propertyMap)
      .map(([bodyField, predicate]) => {
        const v = b[bodyField];
        if (v === undefined || v === null || v === '') return '';
        const s = typeof v === 'string' ? v : (typeof v === 'number' || typeof v === 'boolean') ? String(v) : '';
        if (!s) return '';
        return serializeTriple(entityUri, resolveDescriptor(predicate), s);
      })
      .filter(Boolean)
      .join(' ');

    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a ${spec.typeClass} ; rdfs:label "${escapeLiteral(label)}" . <${sdUri}> ${spec.hasPredicate} <${entityUri}> . ${propTriples} } }`;

    await deps.sparqlUpdate(update);

    const responseData: Record<string, unknown> = {
      subdomain: subdomainId,
      uri: entityUri,
      label,
    };
    for (const field of Object.keys(spec.propertyMap)) {
      responseData[field] = b[field] ?? null;
    }

    return {
      status: 200,
      body: deps.envelope(spec.envelopeName, responseData, now() - start),
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: deps.envelope(spec.envelopeName, { error: message }, now() - start, { error: true }),
    };
  }
}

export const createServiceSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-service-create',
  uriSegment: 'service',
  typeClass: 'chorus:Service',
  hasPredicate: 'chorus:hasService',
  propertyMap: {
    type: 'serviceType',
    host: 'serviceHost',
    status: 'serviceStatus',
    health_endpoint: 'healthEndpoint',
  },
};

export const createPipelineSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-pipeline-create',
  uriSegment: 'pipeline',
  typeClass: 'chorus:Pipeline',
  hasPredicate: 'chorus:hasPipeline',
  propertyMap: {
    source: 'pipelineSource',
    harvester: 'pipelineHarvester',
    icd: 'pipelineICD',
    status: 'pipelineStatus',
    last_run: 'pipelineLastRun',
  },
};

export const createLogSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-log-create',
  uriSegment: 'log',
  typeClass: 'chorus:LogSource',
  hasPredicate: 'chorus:hasLogSource',
  propertyMap: {
    location: 'logSourceLocation',
    retention: 'logSourceRetention',
    status: 'logSourceStatus',
  },
};

export const createGapSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-gap-create',
  uriSegment: 'gap',
  typeClass: 'chorus:Gap',
  hasPredicate: 'chorus:hasGap',
  propertyMap: {
    type: 'gapType',
    description: 'gapDescription',
    severity: 'gapSeverity',
  },
};

export const createPageSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-page-create',
  uriSegment: 'page',
  typeClass: 'chorus:Page',
  hasPredicate: 'chorus:hasPage',
  propertyMap: {
    route: 'pageRoute',
    description: 'pageDescription',
    status: 'pageStatus',
  },
};

export const createIntegrationSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-integration-create',
  uriSegment: 'integration',
  typeClass: 'chorus:Integration',
  hasPredicate: 'chorus:hasIntegration',
  propertyMap: {
    source: 'integrationSource',
    path: 'integrationPath',
    status: 'integrationStatus',
  },
};

export const createPersistenceSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-persistence-create',
  uriSegment: 'store', // existing URI convention uses 'store', not 'persistence'
  typeClass: 'chorus:PersistenceStore',
  hasPredicate: 'chorus:hasPersistence',
  propertyMap: {
    type: 'storeType',
    namespace: 'storeNamespace',
    records: 'storeRecordCount',
    status: 'storeStatus',
  },
};

export const createScenarioSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-scenario-create',
  uriSegment: 'scenario',
  typeClass: 'chorus:Scenario',
  hasPredicate: 'chorus:hasScenario',
  propertyMap: {
    given: 'scenarioGiven',
    when: 'scenarioWhen',
    then: 'scenarioThen',
    notes: 'scenarioNotes',
  },
};

export const createSubdomainService = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createServiceSpec);
export const createSubdomainPipeline = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createPipelineSpec);
export const createSubdomainLog = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createLogSpec);
export const createSubdomainGap = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createGapSpec);
export const createSubdomainPage = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createPageSpec);
export const createSubdomainIntegration = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createIntegrationSpec);
export const createSubdomainPersistence = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createPersistenceSpec);
export const createSubdomainScenario = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createScenarioSpec);

// --- PUT update: DELETE-then-INSERT against a caller-supplied entity URI ---

export interface UpdateEntitySpec {
  envelopeName: string;       // 'service-update'
  typeClass: string;          // 'chorus:Service'
  hasPredicate: string;       // 'chorus:hasService'
  /** Same shape as CreateEntitySpec.propertyMap. */
  propertyMap: Record<string, string | PropertyDescriptor>;
  /** Optional normalization on the request body before triples are built. */
  normalize?: (body: Record<string, unknown>) => Record<string, unknown>;
}

export async function updateSubdomainEntity(
  deps: WriteDeps,
  subdomainId: string,
  entityId: string,
  body: Record<string, unknown> | null | undefined,
  spec: UpdateEntitySpec,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const rawBody = (body || {}) as Record<string, unknown>;
    const b = (spec.normalize ? spec.normalize(rawBody) : rawBody) as Record<string, unknown>;
    const label = b.label;
    if (!label || typeof label !== 'string') {
      return {
        status: 400,
        body: deps.envelope(spec.envelopeName, { error: 'Missing required field: label' }, now() - start, { error: true }),
      };
    }
    const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;
    const entityUri = `https://jeffbridwell.com/chorus#${entityId}`;

    const deleteQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await deps.sparqlUpdate(deleteQuery);

    const propTriples = Object.entries(spec.propertyMap)
      .map(([bodyField, predicate]) => {
        const v = b[bodyField];
        if (v === null || v === undefined || v === '') return '';
        const s = typeof v === 'string' ? v : (typeof v === 'number' || typeof v === 'boolean') ? String(v) : '';
        if (!s) return '';
        return serializeTriple(entityUri, resolveDescriptor(predicate), s);
      })
      .filter(Boolean)
      .join(' ');

    const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { <${entityUri}> a ${spec.typeClass} ; rdfs:label "${escapeLiteral(label)}" . <${sdUri}> ${spec.hasPredicate} <${entityUri}> . ${propTriples} } }`;
    await deps.sparqlUpdate(insert);

    const responseData: Record<string, unknown> = {
      subdomain: subdomainId,
      uri: entityUri,
      label,
    };
    for (const field of Object.keys(spec.propertyMap)) {
      const v = b[field];
      responseData[field] = v === undefined || v === null || v === '' ? null : v;
    }

    return {
      status: 200,
      body: deps.envelope(spec.envelopeName, responseData, now() - start),
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: deps.envelope(spec.envelopeName, { error: message }, now() - start, { error: true }),
    };
  }
}

export const updateServiceSpec: UpdateEntitySpec = {
  envelopeName: 'service-update',
  typeClass: 'chorus:Service',
  hasPredicate: 'chorus:hasService',
  propertyMap: createServiceSpec.propertyMap,
};
export const updatePipelineSpec: UpdateEntitySpec = {
  envelopeName: 'pipeline-update',
  typeClass: 'chorus:Pipeline',
  hasPredicate: 'chorus:hasPipeline',
  propertyMap: createPipelineSpec.propertyMap,
};
export const updateLogSpec: UpdateEntitySpec = {
  envelopeName: 'log-update',
  typeClass: 'chorus:LogSource',
  hasPredicate: 'chorus:hasLogSource',
  propertyMap: createLogSpec.propertyMap,
};
export const updateGapSpec: UpdateEntitySpec = {
  envelopeName: 'gap-update',
  typeClass: 'chorus:Gap',
  hasPredicate: 'chorus:hasGap',
  propertyMap: createGapSpec.propertyMap,
};
export const updatePageSpec: UpdateEntitySpec = {
  envelopeName: 'page-update',
  typeClass: 'chorus:Page',
  hasPredicate: 'chorus:hasPage',
  propertyMap: {
    route: 'pageRoute',
    description: 'pageDescription',
    status: 'pageStatus',
  },
};
export const updateIntegrationSpec: UpdateEntitySpec = {
  envelopeName: 'integration-update',
  typeClass: 'chorus:Integration',
  hasPredicate: 'chorus:hasIntegration',
  propertyMap: {
    source: 'integrationSource',
    path: 'integrationPath',
    status: 'integrationStatus',
  },
};
export const updatePersistenceSpec: UpdateEntitySpec = {
  envelopeName: 'persistence-update',
  typeClass: 'chorus:PersistenceStore',
  hasPredicate: 'chorus:hasPersistence',
  propertyMap: {
    type: 'storeType',
    namespace: 'storeNamespace',
    records: 'storeRecordCount',
    status: 'storeStatus',
  },
};

// --- DELETE: generic section-aware entity removal (#1929) ---

/** Maps URL section slug → (hasPredicate short name, class name). */
export const ENTITY_SECTIONS: Partial<Record<string, { hasProperty: string; class: string }>> = {
  actors: { hasProperty: 'hasActor', class: 'Actor' },
  scenarios: { hasProperty: 'hasScenario', class: 'Scenario' },
  contract: { hasProperty: 'hasContract', class: 'Contract' },
  'prior-art': { hasProperty: 'hasPriorArt', class: 'PriorArt' },
  pages: { hasProperty: 'hasPage', class: 'Page' },
  integrations: { hasProperty: 'hasIntegration', class: 'Integration' },
  persistence: { hasProperty: 'hasPersistence', class: 'PersistenceStore' },
  services: { hasProperty: 'hasService', class: 'Service' },
  pipeline: { hasProperty: 'hasPipeline', class: 'Pipeline' },
  logs: { hasProperty: 'hasLogSource', class: 'LogSource' },
  gaps: { hasProperty: 'hasGap', class: 'Gap' },
};

export async function deleteSubdomainEntity(
  deps: WriteDeps,
  subdomainId: string,
  section: string,
  entityId: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  const sectionMeta = ENTITY_SECTIONS[section];
  if (!sectionMeta) {
    return {
      status: 400,
      body: deps.envelope('entity-delete', { error: `Unknown section: ${section}` }, now() - start, { error: true }),
    };
  }
  try {
    const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;
    const entityUri = `https://jeffbridwell.com/chorus#${entityId}`;
    const update = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . <${sdUri}> chorus:${sectionMeta.hasProperty} <${entityUri}> . } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o . } }`;
    await deps.sparqlUpdate(update);
    // Empty body for 204 signal (adapter converts to .send() with no content).
    return { status: 204, body: null };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: deps.envelope('entity-delete', { error: message }, now() - start, { error: true }),
    };
  }
}

export const updateSubdomainService = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateServiceSpec);
export const updateSubdomainPipeline = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updatePipelineSpec);
export const updateSubdomainLog = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateLogSpec);
export const updateSubdomainGap = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateGapSpec);
export const updateSubdomainPage = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updatePageSpec);
export const updateSubdomainIntegration = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateIntegrationSpec);
export const updateSubdomainPersistence = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updatePersistenceSpec);

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

// --- Heterogeneous-property kinds: actor (URI ref), contract (path alias), prior-art (rdfs:comment) ---

const normalizeContractBody = (b: Record<string, unknown>): Record<string, unknown> => {
  // contract accepts `endpoint` as an alias for `path` (legacy).
  if ((b.endpoint && !b.path)) return { ...b, path: b.endpoint };
  return b;
};

export const createActorSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-actor-create',
  uriSegment: 'actor',
  typeClass: 'chorus:Actor',
  hasPredicate: 'chorus:hasActor',
  propertyMap: {
    role: { predicate: 'chorus:actorRole', kind: 'uri' },
    action: 'actorAction',
  },
};

export const createContractSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-contract-create',
  uriSegment: 'contract',
  typeClass: 'chorus:Contract',
  hasPredicate: 'chorus:hasContract',
  normalize: normalizeContractBody,
  propertyMap: {
    path: { predicate: 'chorus:endpoint' },
    method: { predicate: 'chorus:httpMethod' },
    description: { predicate: 'chorus:contractDescription' },
  },
};

export const createPriorArtSpec: CreateEntitySpec = {
  envelopeName: 'subdomain-prior-art-create',
  uriSegment: 'prior-art',
  typeClass: 'chorus:PriorArt',
  hasPredicate: 'chorus:hasPriorArt',
  propertyMap: {
    path: 'filePath',
    description: { predicate: 'rdfs:comment' },
  },
};

export const updateActorSpec: UpdateEntitySpec = {
  envelopeName: 'actor-update',
  typeClass: 'chorus:Actor',
  hasPredicate: 'chorus:hasActor',
  propertyMap: createActorSpec.propertyMap,
};

export const updateScenarioSpec: UpdateEntitySpec = {
  envelopeName: 'scenario-update',
  typeClass: 'chorus:Scenario',
  hasPredicate: 'chorus:hasScenario',
  propertyMap: createScenarioSpec.propertyMap,
};

export const updateContractSpec: UpdateEntitySpec = {
  envelopeName: 'contract-update',
  typeClass: 'chorus:Contract',
  hasPredicate: 'chorus:hasContract',
  normalize: normalizeContractBody,
  propertyMap: createContractSpec.propertyMap,
};

export const updatePriorArtSpec: UpdateEntitySpec = {
  envelopeName: 'prior-art-update',
  typeClass: 'chorus:PriorArt',
  hasPredicate: 'chorus:hasPriorArt',
  propertyMap: createPriorArtSpec.propertyMap,
};

export const createSubdomainActor = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createActorSpec);
export const createSubdomainContract = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createContractSpec);
export const createSubdomainPriorArt = (deps: WriteDeps, id: string, body: Record<string, unknown> | null | undefined) =>
  createSubdomainEntity(deps, id, body, createPriorArtSpec);

export const updateSubdomainActor = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateActorSpec);
export const updateSubdomainScenario = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateScenarioSpec);
export const updateSubdomainContract = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updateContractSpec);
export const updateSubdomainPriorArt = (deps: WriteDeps, id: string, entityId: string, body: Record<string, unknown> | null | undefined) =>
  updateSubdomainEntity(deps, id, entityId, body, updatePriorArtSpec);
