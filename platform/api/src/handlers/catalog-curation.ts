/**
 * catalog-curation — write API for doc-catalog 5-field tags + lineage edges (#2549).
 *
 * Persists curated tags and doc-to-doc lineage in Athena's urn:chorus:instances
 * graph, following the #2314 Loom Principles write-API pattern. Pure functions
 * + dependency injection so tests run without Fuseki.
 *
 * Endpoints (registered in server.ts):
 *   POST /api/chorus/catalog/tags          — write five-field tag for a doc
 *   POST /api/chorus/catalog/lineage       — write a lineage edge between two docs
 *   GET  /api/chorus/catalog/doc/:hrefb64  — read tags + bidirectional lineage
 *   GET  /api/chorus/catalog/drift         — list path-implied vs persisted divergences
 *
 * Persisted shape:
 *   <doc-uri> a chorus:CatalogDoc ;
 *     chorus:catalogHref "<href>" ;
 *     chorus:product / chorus:subproduct / chorus:domain / chorus:subdomain / chorus:role "<value>" ;
 *     chorus:curatedAt "<ISO>"^^xsd:dateTime .
 *
 *   <subj-uri> chorus:supersedes <obj-uri> .
 *   <subj-uri> chorus:derivedFrom <obj-uri> .
 *   <subj-uri> chorus:reshapedInto <obj-uri> .
 */

import { inferTags, SUBPRODUCT_DOMAINS, GATHERING_SUBDOMAINS } from './doc-tagger';

const ATHENA_INSTANCES = 'urn:chorus:instances';
const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

export type LineagePredicate = 'supersedes' | 'derivedFrom' | 'reshapedInto';
const VALID_PREDICATES: ReadonlySet<LineagePredicate> = new Set(['supersedes', 'derivedFrom', 'reshapedInto']);

export interface CatalogTags {
  product?: string;
  subproduct?: string;
  domain?: string;
  subdomain?: string;
  role?: string;
}

export interface LineageEdge {
  subject_href: string;
  predicate: LineagePredicate;
  object_href: string;
}

export interface DocDetail {
  href: string;
  tags: CatalogTags;
  lineage: { in: LineageEdge[]; out: LineageEdge[] };
}

export interface DriftEntry {
  href: string;
  path_implied: CatalogTags;
  persisted: CatalogTags;
  divergence_fields: string[];
}

export interface CurationDeps {
  sparqlQuery: (q: string) => Promise<unknown>;
  sparqlUpdate: (q: string) => Promise<unknown>;
  envelope: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
  emitSpine?: (event: string, fields: Record<string, string>) => void;
  now?: () => Date;
}

export interface HandlerResult {
  status: number;
  body: unknown;
}

const VALID_PRODUCTS = new Set(['chorus', 'gathering', 'akasha']);
const VALID_SUBPRODUCTS = new Set(['loom', 'werk', 'athena', 'convergence', 'clearing', 'quality', 'borg']);
const VALID_ROLES = new Set(['wren', 'silas', 'kade', 'jeff']);

function buildValidSubdomains(): Set<string> {
  const s = new Set<string>();
  for (const list of Object.values(SUBPRODUCT_DOMAINS)) for (const d of list) s.add(d);
  for (const d of GATHERING_SUBDOMAINS) s.add(d);
  return s;
}
const VALID_SUBDOMAINS = buildValidSubdomains();

function escapeLiteral(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
}

function hrefToUri(href: string): string {
  const id = Buffer.from(href, 'utf8').toString('base64url');
  return `${CHORUS_PREFIX}catalog-doc-${id}`;
}

function uriToHref(uri: string): string {
  const prefix = `${CHORUS_PREFIX}catalog-doc-`;
  if (!uri.startsWith(prefix)) return '';
  const id = uri.slice(prefix.length);
  try {
    return Buffer.from(id, 'base64url').toString('utf8');
  } catch {
    return '';
  }
}

export function decodeHrefId(hrefb64: string): string {
  try {
    return Buffer.from(hrefb64, 'base64url').toString('utf8');
  } catch {
    return '';
  }
}

function pathImpliedTags(href: string): CatalogTags {
  const inferred = inferTags({
    sourcePath: href,
    basename: href.split('/').filter(Boolean).pop() ?? '',
  });
  const out: CatalogTags = {};
  if (inferred.product) out.product = inferred.product;
  if (inferred.subproduct) out.subproduct = inferred.subproduct;
  if (inferred.subdomain) out.subdomain = inferred.subdomain;
  return out;
}

function divergence(implied: CatalogTags, persisted: CatalogTags): string[] {
  const fields: string[] = [];
  if (implied.product && persisted.product && implied.product !== persisted.product) fields.push('product');
  if (implied.subproduct && persisted.subproduct && implied.subproduct !== persisted.subproduct) fields.push('subproduct');
  if (implied.subdomain && persisted.subdomain && implied.subdomain !== persisted.subdomain) fields.push('subdomain');
  return fields;
}

type TagValidationError = { field: string; value: string };

function validateOneTag(value: unknown, field: string, valid: Set<string>): { ok: true; value: string } | { ok: false; err: TagValidationError } {
  if (typeof value !== 'string') return { ok: false, err: { field, value: '<non-string>' } };
  if (!valid.has(value)) return { ok: false, err: { field, value } };
  return { ok: true, value };
}

export function validateTags(body: unknown): { ok: true; tags: CatalogTags & { href: string } } | { ok: false; error: string } {
  if (!body || typeof body !== 'object') return { ok: false, error: 'body must be an object' };
  const b = body as Record<string, unknown>;
  if (typeof b.href !== 'string' || !b.href) return { ok: false, error: 'href is required' };

  const tags: CatalogTags & { href: string } = { href: b.href };
  const checks: Array<[unknown, string, Set<string>, (v: string) => void]> = [
    [b.product, 'product', VALID_PRODUCTS, (v) => { tags.product = v; }],
    [b.subproduct, 'subproduct', VALID_SUBPRODUCTS, (v) => { tags.subproduct = v; }],
    [b.subdomain, 'subdomain', VALID_SUBDOMAINS, (v) => { tags.subdomain = v; }],
    [b.role, 'role', VALID_ROLES, (v) => { tags.role = v; }],
  ];
  for (const [raw, field, valid, assign] of checks) {
    if (raw === undefined) continue;
    const r = validateOneTag(raw, field, valid);
    if (!r.ok) return { ok: false, error: `unknown ${r.err.field}: ${r.err.value}` };
    assign(r.value);
  }
  if (b.domain !== undefined) {
    if (typeof b.domain !== 'string') return { ok: false, error: 'domain must be a string' };
    tags.domain = b.domain;
  }
  return { ok: true, tags };
}

function buildTagInsertTriples(uri: string, href: string, isoNow: string, tags: CatalogTags): string[] {
  const triples = [
    `<${uri}> a chorus:CatalogDoc`,
    `<${uri}> chorus:catalogHref "${escapeLiteral(href)}"`,
    `<${uri}> chorus:curatedAt "${isoNow}"^^xsd:dateTime`,
  ];
  if (tags.product) triples.push(`<${uri}> chorus:product "${escapeLiteral(tags.product)}"`);
  if (tags.subproduct) triples.push(`<${uri}> chorus:subproduct "${escapeLiteral(tags.subproduct)}"`);
  if (tags.domain) triples.push(`<${uri}> chorus:domain "${escapeLiteral(tags.domain)}"`);
  if (tags.subdomain) triples.push(`<${uri}> chorus:subdomain "${escapeLiteral(tags.subdomain)}"`);
  if (tags.role) triples.push(`<${uri}> chorus:role "${escapeLiteral(tags.role)}"`);
  return triples;
}

function buildTagUpsertSparql(uri: string, insertTriples: string[]): string {
  return `PREFIX chorus: <${CHORUS_PREFIX}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
WITH <${ATHENA_INSTANCES}>
DELETE {
  <${uri}> chorus:product ?p .
  <${uri}> chorus:subproduct ?sp .
  <${uri}> chorus:domain ?d .
  <${uri}> chorus:subdomain ?sd .
  <${uri}> chorus:role ?r .
  <${uri}> chorus:curatedAt ?ca .
  <${uri}> chorus:catalogHref ?h .
  <${uri}> a chorus:CatalogDoc .
}
INSERT { ${insertTriples.map((t) => t + ' .').join(' ')} }
WHERE {
  OPTIONAL { <${uri}> chorus:product ?p }
  OPTIONAL { <${uri}> chorus:subproduct ?sp }
  OPTIONAL { <${uri}> chorus:domain ?d }
  OPTIONAL { <${uri}> chorus:subdomain ?sd }
  OPTIONAL { <${uri}> chorus:role ?r }
  OPTIONAL { <${uri}> chorus:curatedAt ?ca }
  OPTIONAL { <${uri}> chorus:catalogHref ?h }
}`;
}

export async function writeCatalogTags(deps: CurationDeps, body: unknown): Promise<HandlerResult> {
  const start = Date.now();
  const v = validateTags(body);
  if (!v.ok) {
    return { status: 400, body: deps.envelope('catalog-tag-write', { error: v.error }, Date.now() - start, { error: true }) };
  }
  const { href, ...tags } = v.tags;
  const uri = hrefToUri(href);
  const isoNow = (deps.now ? deps.now() : new Date()).toISOString();
  const insertTriples = buildTagInsertTriples(uri, href, isoNow, tags);
  const update = buildTagUpsertSparql(uri, insertTriples);
  await deps.sparqlUpdate(update);

  if (deps.emitSpine) {
    const parts: string[] = [];
    if (tags.product) parts.push(`product=${tags.product}`);
    if (tags.subproduct) parts.push(`subproduct=${tags.subproduct}`);
    if (tags.domain) parts.push(`domain=${tags.domain}`);
    if (tags.subdomain) parts.push(`subdomain=${tags.subdomain}`);
    if (tags.role) parts.push(`role=${tags.role}`);
    deps.emitSpine('catalog.tag.curated', { href, after: parts.join(',') });
  }

  return {
    status: 200,
    body: deps.envelope('catalog-tag-write', { uri, href, ...tags, curatedAt: isoNow }, Date.now() - start),
  };
}

export async function writeCatalogLineage(deps: CurationDeps, body: unknown): Promise<HandlerResult> {
  const start = Date.now();
  const errEnv = (msg: string) => ({ status: 400, body: deps.envelope('catalog-lineage-write', { error: msg }, Date.now() - start, { error: true }) });

  if (!body || typeof body !== 'object') return errEnv('body must be an object');
  const b = body as Record<string, unknown>;
  if (typeof b.subject_href !== 'string' || !b.subject_href) return errEnv('subject_href required');
  if (typeof b.object_href !== 'string' || !b.object_href) return errEnv('object_href required');
  if (typeof b.predicate !== 'string' || !VALID_PREDICATES.has(b.predicate as LineagePredicate)) {
    return errEnv('predicate must be one of: supersedes, derivedFrom, reshapedInto');
  }
  const subjUri = hrefToUri(b.subject_href);
  const objUri = hrefToUri(b.object_href);
  const pred = b.predicate;

  const update = `PREFIX chorus: <${CHORUS_PREFIX}>
INSERT DATA { GRAPH <${ATHENA_INSTANCES}> { <${subjUri}> chorus:${pred} <${objUri}> } }`;
  await deps.sparqlUpdate(update);

  if (deps.emitSpine) {
    deps.emitSpine('catalog.lineage.linked', { subject: b.subject_href, predicate: pred, object: b.object_href });
  }

  return {
    status: 200,
    body: deps.envelope('catalog-lineage-write', { subject_href: b.subject_href, predicate: pred, object_href: b.object_href }, Date.now() - start),
  };
}

interface SparqlBinding { value: string }
type SparqlRow = Partial<Record<string, SparqlBinding>>;
interface SparqlResults { results: { bindings: SparqlRow[] } }

function readBinding(row: SparqlRow, key: string): string {
  // SPARQL SELECT projection names are static (from our own queries), not user input.
  // eslint-disable-next-line security/detect-object-injection
  const v = row[key];
  return v ? v.value : '';
}

function tagsFromBinding(row: SparqlRow): CatalogTags {
  const tags: CatalogTags = {};
  const product = readBinding(row, 'product');
  const subproduct = readBinding(row, 'subproduct');
  const domain = readBinding(row, 'domain');
  const subdomain = readBinding(row, 'subdomain');
  const role = readBinding(row, 'role');
  if (product) tags.product = product;
  if (subproduct) tags.subproduct = subproduct;
  if (domain) tags.domain = domain;
  if (subdomain) tags.subdomain = subdomain;
  if (role) tags.role = role;
  return tags;
}

function lineageFromBindings(href: string, rows: SparqlRow[]): { in: LineageEdge[]; out: LineageEdge[] } {
  const out: LineageEdge[] = [];
  const incoming: LineageEdge[] = [];
  for (const row of rows) {
    const dir = readBinding(row, 'direction');
    const predUri = readBinding(row, 'predicate');
    const predicate = predUri.replace(CHORUS_PREFIX, '') as LineagePredicate;
    if (!VALID_PREDICATES.has(predicate)) continue;
    const otherHref = uriToHref(readBinding(row, 'other'));
    if (!otherHref) continue;
    if (dir === 'out') out.push({ subject_href: href, predicate, object_href: otherHref });
    else incoming.push({ subject_href: otherHref, predicate, object_href: href });
  }
  return { in: incoming, out };
}

export async function readCatalogDoc(deps: CurationDeps, hrefb64: string): Promise<HandlerResult> {
  const start = Date.now();
  const href = decodeHrefId(hrefb64);
  if (!href) {
    return { status: 400, body: deps.envelope('catalog-doc', { error: 'invalid hrefb64' }, Date.now() - start, { error: true }) };
  }
  const uri = hrefToUri(href);

  const tagsQuery = `PREFIX chorus: <${CHORUS_PREFIX}>
SELECT ?product ?subproduct ?domain ?subdomain ?role WHERE {
  GRAPH <${ATHENA_INSTANCES}> {
    <${uri}> a chorus:CatalogDoc .
    OPTIONAL { <${uri}> chorus:product ?product }
    OPTIONAL { <${uri}> chorus:subproduct ?subproduct }
    OPTIONAL { <${uri}> chorus:domain ?domain }
    OPTIONAL { <${uri}> chorus:subdomain ?subdomain }
    OPTIONAL { <${uri}> chorus:role ?role }
  }
}`;
  const tagsResult = (await deps.sparqlQuery(tagsQuery)) as SparqlResults;
  const tagBindings = tagsResult.results.bindings;
  if (tagBindings.length === 0) {
    return { status: 404, body: deps.envelope('catalog-doc', { error: 'doc not found in catalog', href }, Date.now() - start, { error: true }) };
  }
  const tags = tagsFromBinding(tagBindings[0]);

  const lineageQuery = `PREFIX chorus: <${CHORUS_PREFIX}>
SELECT ?direction ?predicate ?other WHERE {
  GRAPH <${ATHENA_INSTANCES}> {
    {
      VALUES ?predicate { chorus:supersedes chorus:derivedFrom chorus:reshapedInto }
      <${uri}> ?predicate ?other .
      BIND("out" AS ?direction)
    } UNION {
      VALUES ?predicate { chorus:supersedes chorus:derivedFrom chorus:reshapedInto }
      ?other ?predicate <${uri}> .
      BIND("in" AS ?direction)
    }
  }
}`;
  const lineageResult = (await deps.sparqlQuery(lineageQuery)) as SparqlResults;
  const lineage = lineageFromBindings(href, lineageResult.results.bindings);

  const detail: DocDetail = { href, tags, lineage };
  return {
    status: 200,
    body: deps.envelope('catalog-doc', detail, Date.now() - start),
  };
}

export async function readCatalogDrift(deps: CurationDeps): Promise<HandlerResult> {
  const start = Date.now();
  const query = `PREFIX chorus: <${CHORUS_PREFIX}>
SELECT ?href ?product ?subproduct ?subdomain WHERE {
  GRAPH <${ATHENA_INSTANCES}> {
    ?doc a chorus:CatalogDoc ; chorus:catalogHref ?href .
    OPTIONAL { ?doc chorus:product ?product }
    OPTIONAL { ?doc chorus:subproduct ?subproduct }
    OPTIONAL { ?doc chorus:subdomain ?subdomain }
  }
}`;
  const result = (await deps.sparqlQuery(query)) as SparqlResults;
  const drift: DriftEntry[] = [];
  for (const row of result.results.bindings) {
    const href = readBinding(row, 'href');
    if (!href) continue;
    const persisted = tagsFromBinding(row);
    const implied = pathImpliedTags(href);
    const fields = divergence(implied, persisted);
    if (fields.length > 0) drift.push({ href, path_implied: implied, persisted, divergence_fields: fields });
  }
  return { status: 200, body: deps.envelope('catalog-drift', { drift }, Date.now() - start) };
}
