/**
 * GET /api/chorus/domain/:name/dependencies — layered dependency map (#2188).
 *
 * Two layers, two queries:
 *   1. Direct: chorus:consumes / consumedBy edges in the ontology graph
 *   2. Shared: domains sharing borg:Environment instances via usesEnvironment
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
interface SharedBinding {
  otherDomain: BindingValue;
  otherLabel?: BindingValue;
  envName: BindingValue;
}
interface BindingsOf<T> { results: { bindings: T[] } }

type Sparql = (query: string) => Promise<BindingsOf<DirectBinding> | BindingsOf<SharedBinding>>;
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
SELECT ?dir ?other ?label WHERE {
  GRAPH <urn:chorus:instances> {
    { <${sdUri}> chorus:consumes ?other . BIND("consumes" AS ?dir) }
    UNION
    { ?other chorus:consumes <${sdUri}> . BIND("consumedBy" AS ?dir) }
  }
  OPTIONAL { GRAPH <urn:chorus:ontology> { ?other rdfs:label ?label } }
}`;
    const directResult = (await deps.sparql(directQuery)) as BindingsOf<DirectBinding>;
    const consumes: Array<{ id: string; label: string }> = [];
    const consumedBy: Array<{ id: string; label: string }> = [];
    for (const b of directResult.results.bindings) {
      const id = b.other.value.split('#').pop() || '';
      const entry = { id, label: b.label?.value || id };
      if (b.dir.value === 'consumes') consumes.push(entry);
      else consumedBy.push(entry);
    }

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
        { subdomain: sdId, direct: { consumes, consumedBy }, shared },
        now() - start,
        { direct_count: consumes.length + consumedBy.length, shared_count: shared.length },
      ),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope(
        'domain-dependencies',
        { subdomain: name, direct: { consumes: [], consumedBy: [] }, shared: [] },
        now() - start,
        { direct_count: 0, shared_count: 0 },
      ),
    };
  }
}
