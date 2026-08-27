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
}

export interface AtlasAttribute { name: string; type: string; min: number; max: number | null }
export interface AtlasEdge { name: string; to: string; multiplicity: string; crossDomain: boolean }
export interface AtlasClass { name: string; attributes: AtlasAttribute[]; edges: AtlasEdge[]; parents: string[] }
export interface AtlasDomain { name: string; classes: AtlasClass[] }
export interface Atlas { domains: AtlasDomain[] }

const local = (v: string | undefined): string => String(v || '').split(/[#/]/).pop() || '';

/** min..max as UML multiplicity; missing max is unbounded (*). */
const multiplicity = (min: number, max: number | null): string => `${min}..${max === null ? '*' : max}`;

export function buildClassAtlas(
  rows: SparqlBinding[],
  classHomes: Record<string, string>
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

    if (!row.prop) continue;
    // A blank-node path is an inverse-path constraint — a real requirement
    // that authoring cannot satisfy, so it is SHOWN and labelled, not dropped.
    const name = row.prop.type === 'bnode' ? '(inverse path)' : local(row.prop.value);
    if (!name) continue;

    const min = row.min?.value ? Number(row.min.value) : 0;
    const max = row.max?.value ? Number(row.max.value) : null;
    const rangeClass = local(row.rc?.value);

    if (rangeClass) {
      if (!entry.edges.some((e) => e.name === name && e.to === rangeClass)) {
        const home = classHomes[rangeClass];
        entry.edges.push({
          name, to: rangeClass,
          multiplicity: multiplicity(min, max),
          // cross-domain when the target class's home domain is not this one;
          // an unknown home is treated as cross — an honest "elsewhere".
          crossDomain: home !== dom,
        });
      }
    } else {
      if (!entry.attributes.some((a) => a.name === name)) {
        entry.attributes.push({ name, type: local(row.dt?.value), min, max });
      }
    }
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
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?domain ?class ?prop ?min ?max ?dt ?rc ?parent WHERE { GRAPH <urn:chorus:ontology> {
  ?domain chorus:definesVocabulary ?class .
  OPTIONAL { ?shp sh:targetClass ?class ; sh:property ?b . ?b sh:path ?prop .
    OPTIONAL { ?b sh:minCount ?min } OPTIONAL { ?b sh:maxCount ?max }
    OPTIONAL { ?b sh:datatype ?dt } OPTIONAL { ?b sh:class ?rc } }
  OPTIONAL { ?class rdfs:subClassOf ?parent FILTER(!isBlank(?parent)) }
} }`;

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
      const homes: Record<string, string> = {};
      for (const row of rows) {
        const cls = local(row.class?.value);
        if (cls && !(cls in homes)) homes[cls] = local(row.domain?.value);
      }
      res.json({ storeReachable: true, graph: 'urn:chorus:ontology', ...buildClassAtlas(rows, homes) });
    } catch {
      res.status(502).json({ error: 'store-unreachable', storeReachable: false });
    }
  };
}
