/**
 * Oxigraph-backed SPARQL dep for data-driven regression tests (#2208).
 *
 * Wraps a real (in-process, WASM) SPARQL engine around a TTL fixture.
 * Returns bindings in the SPARQL 1.1 JSON Results shape that Athena handlers
 * already consume: { results: { bindings: [ { varName: { type, value } } ] } }.
 *
 * No Fuseki, no network, no mocks. Tests drive real queries against real data.
 */
import * as fs from 'fs';
import * as path from 'path';
// eslint-disable-next-line @typescript-eslint/no-require-imports
const oxigraph = require('oxigraph') as OxigraphModule;

interface OxigraphTerm {
  termType: 'NamedNode' | 'Literal' | 'BlankNode' | 'DefaultGraph';
  value: string;
  datatype?: OxigraphTerm;
  language?: string;
}

interface OxigraphBinding {
  get(name: string): OxigraphTerm | null;
  size: number;
  [Symbol.iterator](): Iterator<[string, OxigraphTerm]>;
}

interface OxigraphStore {
  load(content: string, options: { format: string; base_iri?: string; to_graph_name?: unknown }): void;
  query(sparql: string): OxigraphBinding[] | boolean | OxigraphQuad[];
  namedNode(value: string): unknown;
}

interface OxigraphQuad {
  subject: OxigraphTerm;
  predicate: OxigraphTerm;
  object: OxigraphTerm;
}

interface OxigraphModule {
  Store: new () => OxigraphStore;
  namedNode(value: string): unknown;
}

export interface SparqlBindingValue { type: string; value: string; datatype?: string; language?: string }
export interface SparqlResultRow { [varName: string]: SparqlBindingValue }
export interface SparqlBindingsResult {
  head: { vars: string[] };
  results: { bindings: SparqlResultRow[] };
}

function termToBinding(t: OxigraphTerm): SparqlBindingValue {
  if (t.termType === 'NamedNode') return { type: 'uri', value: t.value };
  if (t.termType === 'BlankNode') return { type: 'bnode', value: t.value };
  // Literal
  const out: SparqlBindingValue = { type: 'literal', value: t.value };
  if (t.language) out['xml:lang'] = t.language as unknown as string;
  if (t.datatype) out.datatype = t.datatype.value;
  return out;
}

export function loadStoreFromTtl(ttlPath: string, graphName?: string): OxigraphStore {
  const store = new oxigraph.Store();
  const ttl = fs.readFileSync(path.resolve(ttlPath), 'utf-8');
  const opts: { format: string; to_graph_name?: unknown } = { format: 'text/turtle' };
  if (graphName) opts.to_graph_name = oxigraph.namedNode(graphName);
  store.load(ttl, opts);
  return store;
}

/**
 * Build a `sparql(query): Promise<SparqlBindingsResult>` dep backed by a
 * seeded oxigraph Store. Injected into handler tests in place of athenaSparqlQuery.
 */
export function makeSparqlFromStore(store: OxigraphStore): (query: string) => Promise<SparqlBindingsResult> {
  return async (query: string) => {
    const raw = store.query(query);
    if (typeof raw === 'boolean') {
      // ASK query — not expected for these regression tests
      return { head: { vars: [] }, results: { bindings: [] } };
    }
    const bindings: OxigraphBinding[] = raw as OxigraphBinding[];
    const varSet = new Set<string>();
    const rows: SparqlResultRow[] = [];
    for (const b of bindings) {
      const row: SparqlResultRow = {};
      for (const [name, term] of b) {
        varSet.add(name);
        if (term) row[name] = termToBinding(term);
      }
      rows.push(row);
    }
    return { head: { vars: Array.from(varSet) }, results: { bindings: rows } };
  };
}

export function makeSparqlFromTtl(ttlPath: string, graphName?: string): (query: string) => Promise<SparqlBindingsResult> {
  return makeSparqlFromStore(loadStoreFromTtl(ttlPath, graphName));
}
