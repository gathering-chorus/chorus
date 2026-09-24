/**
 * GET /api/chorus/domain/:name/dependencies — layered dependency map (#2188).
 *
 * Three parts:
 *   1. Direct: chorus:dependsOn edges in and out of the domain, read from every
 *      graph (#4293). The legacy chorus:consumes Domain->Domain edges are read too
 *      until #4289 migrates them to dependsOn. Before #4293 this read only
 *      urn:chorus:instances, a graph no domain edge lives in, so every domain
 *      page said "none recorded". The response keys stay consumes/consumedBy
 *      (the page and renderer read them); they now mean depends on / depended on by.
 *   2. Shared: domains sharing borg:Environment instances via usesEnvironment
 *   3. CMDB (#4293): the layer the domain sits in and the services it hosts.
 *
 * Errors degrade to empty envelope (legacy).
 */
import type { FetchResult } from './codebase-topology';

interface BindingValue { value: string }
interface DirectBinding {
  dir: BindingValue;
  other: BindingValue;
  label?: BindingValue;
}
interface LayerBinding {
  layer: BindingValue;
  layerLabel?: BindingValue;
  rank?: BindingValue;
}
interface HostsBinding {
  svc: BindingValue;
  svcLabel?: BindingValue;
}
interface SharedBinding {
  otherDomain: BindingValue;
  otherLabel?: BindingValue;
  envName: BindingValue;
}
interface BindingsOf<T> { results: { bindings: T[] } }

type Sparql = (query: string) => Promise<BindingsOf<DirectBinding> | BindingsOf<SharedBinding> | BindingsOf<LayerBinding> | BindingsOf<HostsBinding>>;
type ResolveSubdomainId = (name: string) => Promise<string>;
type Envelope = (queryName: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;

export interface ChorusDomainDependenciesDeps {
  sparql: Sparql;
  resolveSubdomainId: ResolveSubdomainId;
  envelope: Envelope;
  now?: () => number;
}

type Entry = { id: string; label: string };
const localOf = (iri: string): string => iri.split('#').pop() || '';

/** #4293 — dependsOn both ways from every graph, plus the legacy Domain->Domain consumes until #4289 migrates them. */
async function readDirect(deps: ChorusDomainDependenciesDeps, sdUri: string): Promise<{ consumes: Entry[]; consumedBy: Entry[] }> {
  const q = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?dir ?other (SAMPLE(?l) AS ?label) WHERE {
  { { GRAPH ?g { <${sdUri}> chorus:dependsOn ?other } } UNION { GRAPH ?g { <${sdUri}> chorus:consumes ?other . ?other a chorus:Domain } } BIND("consumes" AS ?dir) }
  UNION
  { { GRAPH ?g { ?other chorus:dependsOn <${sdUri}> } } UNION { GRAPH ?g { ?other chorus:consumes <${sdUri}> . ?other a chorus:Domain } } BIND("consumedBy" AS ?dir) }
  OPTIONAL { GRAPH ?lg { ?other rdfs:label ?l } }
} GROUP BY ?dir ?other ORDER BY ?other`;
  const rows = ((await deps.sparql(q)) as BindingsOf<DirectBinding>).results.bindings;
  const out = { consumes: [] as Entry[], consumedBy: [] as Entry[] };
  for (const b of rows) {
    const id = localOf(b.other.value);
    const list = b.dir.value === 'consumes' ? out.consumes : out.consumedBy;
    if (!list.some((e) => e.id === id)) list.push({ id, label: b.label?.value || id });
  }
  return out;
}

/** #4293 — the one layer the domain sits in, or null. */
async function readLayer(deps: ChorusDomainDependenciesDeps, sdUri: string): Promise<{ id: string; label: string; rank: number | null } | null> {
  const q = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?layer (SAMPLE(?l) AS ?layerLabel) (SAMPLE(?r) AS ?rank) WHERE {
  GRAPH ?g { <${sdUri}> chorus:inLayer ?layer }
  OPTIONAL { GRAPH ?h { ?layer rdfs:label ?l } }
  OPTIONAL { GRAPH ?h2 { ?layer chorus:layerRank ?r } }
} GROUP BY ?layer`;
  const rows = ((await deps.sparql(q)) as BindingsOf<LayerBinding>).results.bindings;
  if (rows.length === 0) return null;
  const b = rows[0];
  const id = localOf(b.layer.value);
  return { id, label: b.layerLabel?.value || id, rank: b.rank ? Number(b.rank.value) : null };
}

/** #4293 — the services the domain hosts. */
async function readHosts(deps: ChorusDomainDependenciesDeps, sdUri: string): Promise<Entry[]> {
  const q = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?svc (SAMPLE(?l) AS ?svcLabel) WHERE {
  GRAPH ?g { <${sdUri}> chorus:hosts ?svc }
  OPTIONAL { GRAPH ?h { ?svc rdfs:label ?l } }
} GROUP BY ?svc ORDER BY ?svc`;
  return ((await deps.sparql(q)) as BindingsOf<HostsBinding>).results.bindings.map((b) => {
    const id = localOf(b.svc.value);
    return { id, label: b.svcLabel?.value || id };
  });
}

/** Domains sharing a borg:Environment with this one. */
async function readShared(deps: ChorusDomainDependenciesDeps, sdUri: string): Promise<Array<{ domain: string; label: string; sharedVia: string[] }>> {
  const q = `PREFIX borg: <urn:borg:ontology/>
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?otherDomain ?otherLabel ?envName WHERE {
  GRAPH <urn:borg:instances> {
    <${sdUri}> borg:usesEnvironment ?env .
    ?otherDomain borg:usesEnvironment ?env .
    ?env borg:environmentName ?envName .
    FILTER(?otherDomain != <${sdUri}>)
  }
  OPTIONAL { GRAPH <urn:chorus:ontology> { ?otherDomain rdfs:label ?otherLabel } }
}`;
  const sharedMap = new Map<string, { domain: string; label: string; sharedVia: string[] }>();
  for (const b of ((await deps.sparql(q)) as BindingsOf<SharedBinding>).results.bindings) {
    const domId = localOf(b.otherDomain.value);
    const entry = sharedMap.get(domId) || { domain: domId, label: b.otherLabel?.value || domId, sharedVia: [] };
    sharedMap.set(domId, entry);
    if (!entry.sharedVia.includes(b.envName.value)) entry.sharedVia.push(b.envName.value);
  }
  return Array.from(sharedMap.values());
}

export async function fetchChorusDomainDependencies(
  deps: ChorusDomainDependenciesDeps,
  name: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const direct = await readDirect(deps, sdUri);
    const shared = await readShared(deps, sdUri);
    const layer = await readLayer(deps, sdUri);
    const hosts = await readHosts(deps, sdUri);
    return {
      status: 200,
      body: deps.envelope(
        'domain-dependencies',
        { subdomain: sdId, direct, shared, layer, hosts },
        now() - start,
        { direct_count: direct.consumes.length + direct.consumedBy.length, shared_count: shared.length, graph: 'all (GRAPH ?g)' },
      ),
    };
  } catch (err) {
    // #4293 — still an empty envelope (the page renders it), but it says it failed,
    // so an empty fold can never be mistaken for "no dependencies".
    return {
      status: 200,
      body: deps.envelope(
        'domain-dependencies',
        { subdomain: name, direct: { consumes: [], consumedBy: [] }, shared: [], layer: null, hosts: [] },
        now() - start,
        { direct_count: 0, shared_count: 0, error: true, message: err instanceof Error ? err.message : String(err) },
      ),
    };
  }
}
