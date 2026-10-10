// @domain: domains
/**
 * #4419 AC1 — blast radius from the live graph. The edges are the model's own:
 * a Product hasDomain / partOf / consumes, a Domain hosts a Service.
 *
 * - Product: dependents = its domains; consumers = the products it is part of
 *   and every product that consumes one of its domains.
 * - Domain:  consumers = products made of it (hasDomain) or consuming it, and
 *   the consumers of the services it hosts; hosts = those services.
 * - Service: consumers = products consuming it.
 * null when the graph holds no such subject: not-found, never an empty answer.
 * #4235 — a Domain with no edges yet still answers (it exists, so its radius is
 * empty, not not-found), and the answer names the FILES the crawl tagged with
 * the domain (CodeFile hasDomain): a product's files are its domains' files.
 */
const C = 'https://jeffbridwell.com/chorus#';

export interface Edge { s: string; kind: string; p: string; o: string }
export interface GraphBlastRadius { iri: string; consumers: string[]; dependents: string[]; hosts: string[]; files: string[] }

const short = (iri: string): string => (iri.startsWith(C) ? `chorus:${iri.slice(C.length)}` : iri);
const full = (iri: string): string => (iri.startsWith('chorus:') ? C + iri.slice('chorus:'.length) : iri);
const tail = (s: string): string => s.slice(s.lastIndexOf('#') + 1);

type Sets = { consumers: Set<string>; dependents: Set<string>; hosts: Set<string> };
const into = (edges: Edge[], p: string, o: string): string[] => edges.filter((e) => e.p === p && e.o === o).map((e) => e.s);

function productRadius(edges: Edge[], me: string, mine: Edge[], r: Sets): void {
  for (const e of mine) {
    if (e.p === 'hasDomain') r.dependents.add(e.o);
    if (e.p === 'partOf') r.consumers.add(e.o);
  }
  for (const d of r.dependents) for (const s of into(edges, 'consumes', d)) if (s !== me) r.consumers.add(s);
}

function domainRadius(edges: Edge[], me: string, mine: Edge[], r: Sets): void {
  for (const s of [...into(edges, 'hasDomain', me), ...into(edges, 'consumes', me)]) r.consumers.add(s);
  for (const e of mine) if (e.p === 'hosts') r.hosts.add(e.o);
  for (const h of r.hosts) for (const s of into(edges, 'consumes', h)) r.consumers.add(s);
}

/** One crawled file and the domain it is tagged with. */
export interface FileTag { domain: string; path: string }

export function blastRadiusFromEdges(edges: Edge[], iri: string, fileTags: FileTag[] = []): GraphBlastRadius | null {
  const me = full(iri);
  const mine = edges.filter((e) => e.s === me);
  if (!mine.length && !edges.some((e) => e.o === me)) return null;
  // a subject with no edges of its own is only ever pointed at: a Domain
  const kind = mine[0]?.kind ?? 'Domain';
  const r: Sets = { consumers: new Set(), dependents: new Set(), hosts: new Set() };
  if (kind === 'Product') productRadius(edges, me, mine, r);
  else if (kind === 'Service') for (const s of into(edges, 'consumes', me)) r.consumers.add(s);
  else domainRadius(edges, me, mine, r);
  const tagged = kind === 'Product' ? r.dependents : kind === 'Service' ? new Set<string>() : new Set([me]);
  const files = [...new Set(fileTags.filter((f) => tagged.has(f.domain)).map((f) => f.path))].sort();
  return {
    iri: short(me),
    consumers: [...r.consumers].map(short),
    dependents: [...r.dependents].map(short),
    hosts: [...r.hosts].map(short),
    files,
  };
}

/** The edges, one read of the store (Fuseki CSV). */
/** #4235 — every Product, Domain and Service, with its edges when it has any
 *  (an edge-less row has empty ?p ?o, so a new domain is found, not not-found). */
export function blastRadiusEdgesQuery(): string {
  return `PREFIX c: <${C}> SELECT ?s ?k ?p ?o WHERE { GRAPH ?g { ?s a ?k .`
    + ' FILTER(?k IN (c:Product, c:Domain, c:Service))'
    + ' OPTIONAL { ?s ?p ?o . FILTER(?p IN (c:hasDomain, c:partOf, c:consumes, c:hosts)) } } }';
}

/** #4235 — the crawl's file tags: every CodeFile and the domain it carries. */
export function fileTagsQuery(): string {
  return `PREFIX c: <${C}> SELECT ?d ?path WHERE { GRAPH ?g { ?f a c:CodeFile ; c:hasDomain ?d ; c:filePath ?path } }`;
}

export function fileTagsFromCsv(csv: string): FileTag[] {
  return csv.split('\n').slice(1).map((l) => l.replace(/\r$/, '').split(',')).filter((c) => c.length >= 2 && c[1] !== '')
    .map(([domain, ...rest]) => ({ domain, path: rest.join(',') }));
}

export function edgesFromCsv(csv: string): Edge[] {
  return csv.split('\n').slice(1).map((l) => l.replace(/\r$/, '').split(',')).filter((c) => c.length >= 4)
    .map(([s, k, p, o]) => ({ s, kind: tail(k), p: tail(p), o }));
}
