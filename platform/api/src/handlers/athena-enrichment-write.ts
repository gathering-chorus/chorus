/**
 * POST endpoints for envelope enrichment writes (#2206).
 *
 * Problem: #2178 shipped live-graph SPARQL mutations that didn't survive
 * Fuseki rebuild. This handler family is the durable path — every accepted
 * write goes BOTH to Fuseki (immediate read availability) AND to a checked-in
 * TTL seed (durability). Fuseki loads the seed at init; rebuild → full state.
 *
 * Deps injected:
 *   sparqlUpdate(update)  — live-graph INSERT
 *   appendSeed(triple)    — appends a single TTL triple line to the seed file
 *   now?                  — default Date.now
 */
import type { FetchResult } from './codebase-topology';

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';
const INSTANCES_GRAPH = 'urn:chorus:instances';

const VALID_ID = /^[a-z0-9][a-z0-9._-]*$/i;
const VALID_EDGE = new Set(['reads', 'writes', 'consumes']);

export interface AthenaEnrichmentDeps {
  sparqlUpdate: (update: string) => Promise<void>;
  appendSeed: (triple: string) => void;
  now?: () => number;
}

function sanitizeId(id: string | undefined | null): string | null {
  if (!id || typeof id !== 'string') return null;
  if (!VALID_ID.test(id)) return null;
  return id;
}

function escLiteral(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '');
}

function serviceUri(subdomainId: string, entityId: string): string {
  return `${CHORUS_PREFIX}${subdomainId}-service-${entityId}`;
}

function storeUri(subdomainId: string, entityId: string): string {
  return `${CHORUS_PREFIX}${subdomainId}-store-${entityId}`;
}

interface DescriptionRequest {
  subdomainId: string;
  entityId: string;
  body?: { description?: unknown };
}

interface EdgeRequest {
  subdomainId: string;
  entityId: string;
  predicate: 'reads' | 'writes' | 'consumes';
  body?: { target?: string };
}

async function writeDescription(
  deps: AthenaEnrichmentDeps,
  subjectUri: string,
  description: string,
): Promise<FetchResult> {
  const escaped = escLiteral(description);
  const update = `PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    INSERT DATA { GRAPH <${INSTANCES_GRAPH}> {
      <${subjectUri}> rdfs:comment "${escaped}" .
    } }`;

  try {
    await deps.sparqlUpdate(update);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `SPARQL update failed: ${message}` } };
  }

  deps.appendSeed(`<${subjectUri}> <http://www.w3.org/2000/01/rdf-schema#comment> "${escaped}" .`);
  return { status: 200, body: { ok: true, subject: subjectUri, description } };
}

export async function fetchAthenaServiceDescription(
  deps: AthenaEnrichmentDeps,
  req: DescriptionRequest,
): Promise<FetchResult> {
  const sub = sanitizeId(req.subdomainId);
  const ent = sanitizeId(req.entityId);
  if (!sub || !ent) {
    return { status: 400, body: { error: 'Invalid subdomainId or entityId' } };
  }
  const description = req.body?.description;
  if (typeof description !== 'string' || description.trim().length === 0) {
    return { status: 400, body: { error: 'body.description is required' } };
  }
  return writeDescription(deps, serviceUri(sub, ent), description);
}

export async function fetchAthenaPersistenceDescription(
  deps: AthenaEnrichmentDeps,
  req: DescriptionRequest,
): Promise<FetchResult> {
  const sub = sanitizeId(req.subdomainId);
  const ent = sanitizeId(req.entityId);
  if (!sub || !ent) {
    return { status: 400, body: { error: 'Invalid subdomainId or entityId' } };
  }
  const description = req.body?.description;
  if (typeof description !== 'string' || description.trim().length === 0) {
    return { status: 400, body: { error: 'body.description is required' } };
  }
  return writeDescription(deps, storeUri(sub, ent), description);
}

export async function fetchAthenaServiceEdge(
  deps: AthenaEnrichmentDeps,
  req: EdgeRequest,
): Promise<FetchResult> {
  if (!VALID_EDGE.has(req.predicate)) {
    return { status: 400, body: { error: `predicate must be one of: ${Array.from(VALID_EDGE).join(', ')}` } };
  }
  const sub = sanitizeId(req.subdomainId);
  const ent = sanitizeId(req.entityId);
  const target = sanitizeId(req.body?.target);
  if (!sub || !ent || !target) {
    return { status: 400, body: { error: 'subdomainId, entityId, and body.target are required' } };
  }

  const subjectUri = serviceUri(sub, ent);
  // consumes targets another Service (service-to-service edge);
  // reads/writes target a Persistence store.
  const targetUri = req.predicate === 'consumes' ? serviceUri(sub, target) : storeUri(sub, target);

  const update = `PREFIX chorus: <${CHORUS_PREFIX}>
    INSERT DATA { GRAPH <${INSTANCES_GRAPH}> {
      <${subjectUri}> chorus:${req.predicate} <${targetUri}> .
    } }`;

  try {
    await deps.sparqlUpdate(update);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `SPARQL update failed: ${message}` } };
  }

  deps.appendSeed(`<${subjectUri}> <${CHORUS_PREFIX}${req.predicate}> <${targetUri}> .`);
  return { status: 200, body: { ok: true, subject: subjectUri, predicate: req.predicate, target: targetUri } };
}
