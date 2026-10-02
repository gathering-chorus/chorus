// @domain: domains
/**
 * #4419 AC1 — blast radius from the live graph, not data/athena/tree.json
 * (hand-written, last changed 2026-06-15). The edges are the model's own:
 * a Product hasDomain / partOf / consumes, a Domain hosts a Service.
 *
 * - Product: dependents = its domains; consumers = the products it is part of
 *   and every product that consumes one of its domains.
 * - Domain:  consumers = products made of it (hasDomain) or consuming it, and
 *   the consumers of the services it hosts; hosts = those services.
 * - Service: consumers = products consuming it.
 * null when the graph holds no such subject: not-found, never an empty answer.
 */
const C = 'https://jeffbridwell.com/chorus#';

export interface Edge { s: string; kind: string; p: string; o: string }
export interface GraphBlastRadius { iri: string; consumers: string[]; dependents: string[]; hosts: string[] }

const short = (iri: string): string => (iri.startsWith(C) ? `chorus:${iri.slice(C.length)}` : iri);
const full = (iri: string): string => (iri.startsWith('chorus:') ? C + iri.slice('chorus:'.length) : iri);
const tail = (s: string): string => s.slice(s.lastIndexOf('#') + 1);

export function blastRadiusFromEdges(edges: Edge[], iri: string): GraphBlastRadius | null {
  const me = full(iri);
  const mine = edges.filter((e) => e.s === me);
  if (!mine.length && !edges.some((e) => e.o === me)) return null;
  // a subject with no edges of its own is only ever pointed at: a Domain
  const k = mine[0]?.kind ?? 'Domain';
  const consumers = new Set<string>();
  const dependents = new Set<string>();
  const hosts = new Set<string>();
  const into = (p: string, o: string): string[] => edges.filter((e) => e.p === p && e.o === o).map((e) => e.s);
  if (k === 'Product') {
    for (const e of mine) {
      if (e.p === 'hasDomain') dependents.add(e.o);
      if (e.p === 'partOf') consumers.add(e.o);
    }
    for (const d of dependents) for (const s of into('consumes', d)) if (s !== me) consumers.add(s);
  } else if (k === 'Service') {
    for (const s of into('consumes', me)) consumers.add(s);
  } else {
    for (const s of [...into('hasDomain', me), ...into('consumes', me)]) consumers.add(s);
    for (const e of mine) if (e.p === 'hosts') hosts.add(e.o);
    for (const h of hosts) for (const s of into('consumes', h)) consumers.add(s);
  }
  return {
    iri: short(me),
    consumers: [...consumers].map(short),
    dependents: [...dependents].map(short),
    hosts: [...hosts].map(short),
  };
}

/** The edges, one read of the store (Fuseki CSV). */
export function blastRadiusEdgesQuery(): string {
  return `PREFIX c: <${C}> SELECT ?s ?k ?p ?o WHERE { GRAPH ?g { ?s a ?k ; ?p ?o .`
    + ' FILTER(?k IN (c:Product, c:Domain, c:Service))'
    + ' FILTER(?p IN (c:hasDomain, c:partOf, c:consumes, c:hosts)) } }';
}

export function edgesFromCsv(csv: string): Edge[] {
  return csv.split('\n').slice(1).map((l) => l.replace(/\r$/, '').split(',')).filter((c) => c.length >= 4)
    .map(([s, k, p, o]) => ({ s, kind: tail(k), p: tail(p), o }));
}
