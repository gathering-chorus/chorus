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

export async function fetchChorusDomainDependencies(
  deps: ChorusDomainDependenciesDeps,
  name: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();

  try {
    const sdId = await deps.resolveSubdomainId(name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;

    const directQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?dir ?other WHERE {
  { GRAPH ?g { <${sdUri}> chorus:dependsOn ?other } BIND("consumes" AS ?dir) }
  UNION { GRAPH ?g { ?other chorus:dependsOn <${sdUri}> } BIND("consumedBy" AS ?dir) }
  UNION { GRAPH ?g { <${sdUri}> chorus:consumes ?other . ?other a chorus:Domain } BIND("consumes" AS ?dir) }
  UNION { GRAPH ?g { ?other chorus:consumes <${sdUri}> . ?other a chorus:Domain } BIND("consumedBy" AS ?dir) }
}`;
    const labelOf = async (iris: string[]): Promise<Map<string, string>> => {
      const out = new Map<string, string>();
      if (!iris.length) return out;
      const values = iris.map((i) => `<${i}>`).join(' ');
      const q = `PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?other (SAMPLE(?l) AS ?label) WHERE { VALUES ?other { ${values} } OPTIONAL { GRAPH ?g { ?other rdfs:label ?l } } } GROUP BY ?other`;
      const r = (await deps.sparql(q)) as BindingsOf<DirectBinding>;
      for (const b of r.results.bindings) if (b.label?.value) out.set(b.other.value, b.label.value);
      return out;
    };
    const directResult = (await deps.sparql(directQuery)) as BindingsOf<DirectBinding>;
    const labels = await labelOf([...new Set(directResult.results.bindings.map((b) => b.other.value))]);
    const consumes: Array<{ id: string; label: string }> = [];
    const consumedBy: Array<{ id: string; label: string }> = [];
    for (const b of directResult.results.bindings) {
      const id = b.other.value.split('#').pop() || '';
      const entry = { id, label: labels.get(b.other.value) || id };
      const list = b.dir.value === 'consumes' ? consumes : consumedBy;
      if (!list.some((e) => e.id === id)) list.push(entry);
    }

    // #4293 — the layer and the hosted services, from whichever graph holds them.
    const layerQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?layer (SAMPLE(?l) AS ?layerLabel) (SAMPLE(?r) AS ?rank) WHERE {
  GRAPH ?g { <${sdUri}> chorus:inLayer ?layer }
  OPTIONAL { GRAPH ?h { ?layer rdfs:label ?l } }
  OPTIONAL { GRAPH ?h2 { ?layer chorus:layerRank ?r } }
} GROUP BY ?layer`;
    const layerRows = ((await deps.sparql(layerQuery)) as BindingsOf<LayerBinding>).results.bindings;
    const layer = layerRows.length
      ? {
          id: layerRows[0].layer.value.split('#').pop() || '',
          label: layerRows[0].layerLabel?.value || layerRows[0].layer.value.split('#').pop() || '',
          rank: layerRows[0].rank ? Number(layerRows[0].rank.value) : null,
        }
      : null;
    const hostsQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?svc (SAMPLE(?l) AS ?svcLabel) WHERE {
  GRAPH ?g { <${sdUri}> chorus:hosts ?svc }
  OPTIONAL { GRAPH ?h { ?svc rdfs:label ?l } }
} GROUP BY ?svc ORDER BY ?svc`;
    const hosts = ((await deps.sparql(hostsQuery)) as BindingsOf<HostsBinding>).results.bindings.map((b) => {
      const id = b.svc.value.split('#').pop() || '';
      return { id, label: b.svcLabel?.value || id };
    });

    const sharedQuery = `PREFIX borg: <urn:borg:ontology/>
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
    const sharedResult = (await deps.sparql(sharedQuery)) as BindingsOf<SharedBinding>;

    const sharedMap = new Map<string, { domain: string; label: string; sharedVia: string[] }>();
    for (const b of sharedResult.results.bindings) {
      const domId = b.otherDomain.value.split('#').pop() || '';
      const label = b.otherLabel?.value || domId;
      const env = b.envName.value;
      if (!sharedMap.has(domId)) sharedMap.set(domId, { domain: domId, label, sharedVia: [] });
      const entry = sharedMap.get(domId)!;
      if (!entry.sharedVia.includes(env)) entry.sharedVia.push(env);
    }
    const shared = Array.from(sharedMap.values());

    return {
      status: 200,
      body: deps.envelope(
        'domain-dependencies',
        { subdomain: sdId, direct: { consumes, consumedBy }, shared, layer, hosts },
        now() - start,
        { direct_count: consumes.length + consumedBy.length, shared_count: shared.length, graph: 'all (GRAPH ?g)' },
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
