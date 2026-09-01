/* #3992 — Class Atlas: the OWL model as UML, per domain, straight from the
 * store. One SPARQL query against urn:chorus:ontology returns every
 * (domain, class, property-shape, subclass) row; buildClassAtlas is the pure
 * fold from those rows to the view the page renders. Zero hardcoded model
 * content — a class added through the pen appears on the next load because
 * the query walks definesVocabulary, not a list in this file.
 */
import type { Request, Response } from 'express';

export interface SparqlTerm { type: string; value: string; datatype?: string }
export interface SparqlBinding {
  domain?: SparqlTerm; class?: SparqlTerm; prop?: SparqlTerm;
  min?: SparqlTerm; max?: SparqlTerm; dt?: SparqlTerm; rc?: SparqlTerm; parent?: SparqlTerm;
  // #4053 — one row per sh:in list member (rdf:rest*/rdf:first), so a four-value
  // enum arrives as four rows that must fold into one attribute.
  inValue?: SparqlTerm;
  // #4053 — one row per sh:or alternative. A polymorphic edge (consumes may
  // target a Service OR a Domain) has no sh:class, so reading only sh:class
  // left it invisible — which is exactly why the atlas showed no Product→Service.
  orClass?: SparqlTerm;
}

export interface AtlasAttribute { name: string; type: string; min: number; max: number | null; allowed?: string[] }
export interface AtlasEdge { name: string; to: string; multiplicity: string; crossDomain: boolean }
export interface AtlasClass {
  name: string; attributes: AtlasAttribute[]; edges: AtlasEdge[]; parents: string[];
  // #4053 — DECLARED (in the pen) and SERVED (mounted by athena-make) are two
  // different claims. served=false is the fail-closed default: if discovery
  // cannot be read we say "not confirmed", never "fine".
  served?: boolean;
  // Present only when a count actually resolved. 0 means asked-and-empty;
  // undefined means could-not-ask. Collapsing them rebuilds the ambiguity.
  rowCount?: number;
}

/** One row of athena-make's discovery document, plus the collection's live count. */
export interface ServedInfo { kind: string; collection: string; count?: number }
export interface AtlasDomain { name: string; classes: AtlasClass[] }
export interface Atlas { domains: AtlasDomain[] }

const local = (v: string | undefined): string => String(v || '').split(/[#/]/).pop() || '';

/** min..max as UML multiplicity; missing max is unbounded (*). */
const multiplicity = (min: number, max: number | null): string => `${min}..${max === null ? '*' : max}`;

function addPropertyRow(
  entry: AtlasClass,
  row: SparqlBinding,
  dom: string,
  classHomes: ReadonlyMap<string, string>
): void {
  if (!row.prop) return;
  // A blank-node path is an inverse-path constraint — a real requirement
  // that authoring cannot satisfy, so it is SHOWN and labelled, not dropped.
  const name = row.prop.type === 'bnode' ? '(inverse path)' : local(row.prop.value);
  if (!name) return;

  const min = row.min?.value ? Number(row.min.value) : 0;
  const max = row.max?.value ? Number(row.max.value) : null;
  // #4053 — sh:class OR one sh:or alternative. Both name a target class; a
  // polymorphic edge simply arrives as several rows, one per branch.
  const rangeClass = local(row.rc?.value) || local(row.orClass?.value);

  if (rangeClass) {
    if (entry.edges.some((e) => e.name === name && e.to === rangeClass)) return;
    // cross-domain when the target class's home domain is not this one;
    // an unknown home is treated as cross — an honest "elsewhere".
    entry.edges.push({
      name, to: rangeClass,
      multiplicity: multiplicity(min, max),
      crossDomain: classHomes.get(rangeClass) !== dom,
    });
    return;
  }

  addAttributeRow(entry, name, row, min, max);
}

/** #4053 — sh:in members arrive one row each, so an attribute accumulates its
 *  allowed values across rows. Split out to keep addPropertyRow under the
 *  complexity ratchet. */
function addAttributeRow(
  entry: AtlasClass,
  name: string,
  row: SparqlBinding,
  min: number,
  max: number | null
): void {
  const member = row.inValue?.value;
  const existing = entry.attributes.find((a) => a.name === name);
  if (existing) {
    if (member !== undefined && !existing.allowed?.includes(member)) {
      existing.allowed = [...(existing.allowed ?? []), member];
    }
    return;
  }
  // A shape may constrain values without declaring a datatype (ProductShape's
  // status does exactly that); an enum IS the type in that case.
  const declared = local(row.dt?.value);
  entry.attributes.push({
    name,
    type: declared || (member !== undefined ? 'enum' : ''),
    min,
    max,
    ...(member !== undefined ? { allowed: [member] } : {}),
  });
}

/**
 * #4053 — fold athena-make's discovery document onto the atlas so DECLARED
 * (the pen said this class exists) and SERVED (athena-make mounted it) are
 * two visible claims rather than one assumed one. Jeff reads the atlas to
 * consume the athena-* pipeline; without this a green verb is unfalsifiable.
 *
 * Fail-closed by construction: a class not present in `served` is marked
 * false, so an athena-make outage renders as "not confirmed" and can never
 * render as a clean bill of health.
 */
export function annotateServed(atlas: Atlas, served: readonly ServedInfo[]): Atlas {
  const byKind = new Map(served.map((s) => [s.kind, s]));
  for (const domain of atlas.domains) {
    for (const cls of domain.classes) {
      const hit = byKind.get(cls.name);
      cls.served = hit !== undefined;
      // Only set rowCount when a count actually resolved: 0 is
      // asked-and-empty, undefined is could-not-ask.
      if (hit?.count !== undefined) cls.rowCount = hit.count;
    }
  }
  return atlas;
}

export function buildClassAtlas(
  rows: SparqlBinding[],
  classHomes: ReadonlyMap<string, string>
): Atlas {
  // domain → class → accumulated view. A property can appear in several rows
  // (one per OPTIONAL combination); merge by name, never duplicate.
  const domains = new Map<string, Map<string, AtlasClass>>();

  for (const row of rows) {
    const dom = local(row.domain?.value);
    const cls = local(row.class?.value);
    if (!dom || !cls) continue;

    const classes = domains.get(dom) ?? new Map<string, AtlasClass>();
    domains.set(dom, classes);
    const entry = classes.get(cls) ?? { name: cls, attributes: [], edges: [], parents: [] };
    classes.set(cls, entry);

    // Subclass edge (may ride any row for the class).
    const parent = local(row.parent?.value);
    if (parent && !entry.parents.includes(parent)) entry.parents.push(parent);

    addPropertyRow(entry, row, dom, classHomes);
  }

  return {
    domains: [...domains.entries()]
      .map(([name, classes]) => ({
        name,
        classes: [...classes.values()].sort((a, b) => a.name.localeCompare(b.name)),
      }))
      .sort((a, b) => a.name.localeCompare(b.name)),
  };
}

const ATLAS_QUERY = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?domain ?class ?prop ?min ?max ?dt ?rc ?parent ?inValue ?orClass WHERE { GRAPH <urn:chorus:ontology> {
  ?domain chorus:definesVocabulary ?class .
  OPTIONAL { ?shp sh:targetClass ?class ; sh:property ?b . ?b sh:path ?prop .
    OPTIONAL { ?b sh:minCount ?min } OPTIONAL { ?b sh:maxCount ?max }
    OPTIONAL { ?b sh:datatype ?dt } OPTIONAL { ?b sh:class ?rc }
    OPTIONAL { ?b sh:in ?inList . ?inList rdf:rest*/rdf:first ?inValue }
    OPTIONAL { ?b sh:or ?orList . ?orList rdf:rest*/rdf:first ?alt . ?alt sh:class ?orClass } }
  OPTIONAL { ?class rdfs:subClassOf ?parent FILTER(!isBlank(?parent)) }
} }`;

/**
 * #4053 — ask athena-make what it actually serves, and how many rows each
 * collection holds. Never throws: an unreachable athena-make returns [], which
 * annotateServed reads as "nothing confirmed served" — the fail-closed
 * direction. Counts are fetched in parallel and a failed count is simply
 * absent, so could-not-ask stays distinguishable from asked-and-empty.
 */
export async function fetchServed(base: string): Promise<ServedInfo[]> {
  try {
    const r = await fetch(base + '/');
    if (!r.ok) return [];
    const disco = (await r.json()) as { primitives?: Array<{ kind?: string; collection?: string }> };
    const rows = (disco.primitives ?? []).filter((p) => p.kind && p.collection) as Array<{ kind: string; collection: string }>;
    return await Promise.all(rows.map(async (p): Promise<ServedInfo> => {
      try {
        const c = await fetch(base + p.collection.replace(/^\/v1/, ''));
        if (!c.ok) return { kind: p.kind, collection: p.collection };
        const body = (await c.json()) as { count?: number };
        return typeof body.count === 'number'
          ? { kind: p.kind, collection: p.collection, count: body.count }
          : { kind: p.kind, collection: p.collection };
      } catch {
        return { kind: p.kind, collection: p.collection };
      }
    }));
  } catch {
    return [];
  }
}

export function classAtlasHandler() {
  return async (_req: Request, res: Response): Promise<void> => {
    const endpoint = (process.env.CHORUS_FUSEKI || 'http://localhost:3030/pods') + '/query';
    try {
      const r = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/sparql-results+json' },
        body: 'query=' + encodeURIComponent(ATLAS_QUERY),
      });
      if (!r.ok) {
        // storeReachable is explicit so an empty atlas can never be read as
        // "the model has no classes" when the truth is "we could not ask".
        res.status(502).json({ error: 'store-unreachable', storeReachable: false, http: r.status });
        return;
      }
      const body = (await r.json()) as { results?: { bindings?: SparqlBinding[] } };
      const rows = body.results?.bindings ?? [];
      // class → home domain, derived from the same rows (definesVocabulary).
      const homes = new Map<string, string>();
      for (const row of rows) {
        const cls = local(row.class?.value);
        if (cls && !homes.has(cls)) homes.set(cls, local(row.domain?.value));
      }
      // #4053 — the atlas answers DECLARED (the pen) and SERVED (athena-make)
      // in one payload, so a green verb becomes falsifiable on the page.
      const makeBase = process.env.OWL_API_URL || 'http://localhost:3360';
      const served = await fetchServed(makeBase);
      res.json({
        storeReachable: true,
        graph: 'urn:chorus:ontology',
        servedFrom: makeBase,
        servedReachable: served.length > 0,
        ...annotateServed(buildClassAtlas(rows, homes), served),
      });
    } catch {
      res.status(502).json({ error: 'store-unreachable', storeReachable: false });
    }
  };
}
