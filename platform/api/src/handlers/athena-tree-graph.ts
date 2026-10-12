// @domain: domains
/**
 * #4235 — the Athena structure (products, domains, services) read from the live
 * graph. Jeff, 2026-10-10: "i honestly dont think we want any tree.json
 * dependencies tree.json was our proto graph". This module replaces the
 * hand-written proto-graph file, which #4235 deleted; GET /api/athena/tree
 * (chorus_tree_get) answers from the store, and a store that does not answer
 * is an error, never a file. #4508 — ownership is the model door's
 * links.ownedBy; the second ownership route here was deleted.
 */
import { csvRecords, type CsvQuery } from './nightly-graph';

const C = 'https://jeffbridwell.com/chorus#';
const RDFS = 'http://www.w3.org/2000/01/rdf-schema#';

export interface TreeNode {
  iri: string;
  label: string;
  comment?: string;
  ownedBy?: string;
  status?: string;
  atStep?: string;
  gaps?: string[];
  hasDomain?: string[];
  hasChild?: string[];
  hosts?: string[];
  consumes?: string[];
}
export interface GraphTree { source: 'graph'; products: TreeNode[]; domains: TreeNode[]; services: TreeNode[] }

const short = (v: string): string => (v.startsWith(C) ? `chorus:${v.slice(C.length)}` : v);
const LISTS = new Set(['hasDomain', 'hosts', 'consumes', 'gaps']);
const FIELDS = new Map<string, string>([
  [`${RDFS}label`, 'label'], [`${C}label`, 'label'], [`${RDFS}comment`, 'comment'], [`${C}comment`, 'comment'],
  [`${C}ownedBy`, 'ownedBy'], [`${C}status`, 'status'], [`${C}atStep`, 'atStep'], [`${C}gaps`, 'gaps'],
  [`${C}hasDomain`, 'hasDomain'], [`${C}hosts`, 'hosts'], [`${C}consumes`, 'consumes'], [`${C}partOf`, 'partOf'],
]);
type Building = TreeNode & { kind: string; partOf?: string[] };

/** One field onto a node: list fields collect distinct values, the first label wins. */
function addField(node: Building, f: string, v: string): void {
  if (f === 'partOf' || LISTS.has(f)) {
    const list = (Reflect.get(node, f) as string[] | undefined) ?? [];
    if (!list.includes(v)) list.push(v);
    Reflect.set(node, f, list);
  } else if (!(f === 'label' && node.label)) {
    Reflect.set(node, f, v);
  }
}

/** Every Product, Domain and Service with the fields the tree carries. */
export function structureQuery(): string {
  const preds = [...FIELDS.keys()].map((p) => `<${p}>`).join(', ');
  return `SELECT ?s ?k ?p ?o WHERE { GRAPH ?g { ?s a ?k . FILTER(?k IN (<${C}Product>, <${C}Domain>, <${C}Service>))`
    + ` OPTIONAL { ?s ?p ?o . FILTER(?p IN (${preds})) } } }`;
}

/** Rows → the tree. A product's children are the products partOf it. */
export function treeFromCsv(csv: string): GraphTree {
  const nodes = new Map<string, Building>();
  for (const r of csvRecords(csv)) {
    const iri = short(r.s);
    const node = nodes.get(iri) ?? { iri, label: '', kind: r.k.slice(r.k.lastIndexOf('#') + 1) };
    nodes.set(iri, node);
    const f = FIELDS.get(r.p);
    if (f && r.o !== '') addField(node, f, short(r.o));
  }
  for (const n of nodes.values()) {
    if (!n.label) n.label = n.iri.replace(/^chorus:/, '');
    for (const parent of n.partOf ?? []) {
      const p = nodes.get(parent);
      if (p) (p.hasChild ??= []).push(n.iri);
    }
  }
  const of = (kind: string): TreeNode[] => [...nodes.values()].filter((n) => n.kind === kind)
    .map(({ kind: _k, partOf: _p, ...rest }) => rest).sort((a, b) => a.iri.localeCompare(b.iri));
  return { source: 'graph', products: of('Product'), domains: of('Domain'), services: of('Service') };
}

/** The tree, one read of the store. null = the store did not answer. */
export async function loadTreeFromGraph(query: CsvQuery): Promise<GraphTree | null> {
  const csv = await query(structureQuery());
  return csv === null ? null : treeFromCsv(csv);
}
