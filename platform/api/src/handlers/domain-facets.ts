/**
 * Domain facet handlers (#2173 AC4).
 *
 * Four facet endpoints that share shape: /api/chorus/domain/:name/{tests,
 * logs, services, decisions}. Each resolves the subdomain → queries SPARQL
 * or an upstream service → shapes the result → returns an Athena envelope.
 * On error, returns an empty-result envelope rather than a 500 — these
 * endpoints back domain-detail pages where a partial render is better than
 * a failure screen. That contract stays; this extraction just makes it
 * testable.
 */

import type { FetchResult } from './sessions';
import type { SparqlResult } from './athena-health';
import { resolveDomainIdentity } from './domain-identity';

export interface DomainFacetDeps {
  sparql: (query: string) => Promise<SparqlResult>;
  resolveSubdomainId: (name: string) => Promise<string>;
  envelope: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
  fetcher?: (url: string, init?: RequestInit) => Promise<Response>;
  now?: () => number;
}

// --- tests: upstream HTTP proxy to /api/quality/domain/:d scanner ---

export async function fetchDomainTests(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const fetcher = deps.fetcher ?? fetch;
  const start = now();
  try {
    // #2430: shared resolver. Upstream quality scanner is domain-name-keyed, not
    // URI-keyed — use resolver.primary for the path. Note: upstream scanner has
    // no data for loom subdomains regardless of alias; scanner enhancement is a
    // separate follow-on, not this card.
    const identity = resolveDomainIdentity(subdomainName);
    const domain = identity.primary;
    const upstream = await fetcher(`http://localhost:3000/api/quality/domain/${domain}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!upstream.ok) {
      return {
        status: 200,
        body: deps.envelope('domain-tests', { subdomain: subdomainName, tests: [], byType: {} }, now() - start, { count: 0 }),
      };
    }
    const scanData = (await upstream.json()) as { files?: Array<{ name: string; kind: string }>; total?: number };
    const tests = (scanData.files || []).map((f) => ({ path: f.name, type: f.kind }));
    const byType: Record<string, number> = {};
    for (const t of tests) byType[t.type] = (byType[t.type] || 0) + 1;
    return {
      status: 200,
      body: deps.envelope(
        'domain-tests',
        { subdomain: subdomainName, tests, byType, total: scanData.total || 0 },
        now() - start,
        { count: tests.length },
      ),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-tests', { subdomain: subdomainName, tests: [], byType: {} }, now() - start, { count: 0 }),
    };
  }
}

// --- logs: SPARQL against urn:chorus:instances for chorus:hasLogSource ---

export async function fetchDomainLogs(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?log ?label ?location ?status WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasLogSource ?log . OPTIONAL { ?log rdfs:label ?label } OPTIONAL { ?log chorus:logSourceLocation ?location } OPTIONAL { ?log chorus:logSourceStatus ?status } } }`;
    const result = await deps.sparql(query);
    const logs = result.results.bindings.map((b) => ({
      label: b.label?.value || (b.log?.value || '').split('#').pop() || '',
      location: b.location?.value || null,
      status: b.status?.value || null,
    }));
    return {
      status: 200,
      body: deps.envelope('domain-logs', { subdomain: sdId, logs }, now() - start, { count: logs.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-logs', { subdomain: subdomainName, logs: [] }, now() - start, { count: 0 }),
    };
  }
}

// --- services: SPARQL against urn:chorus:instances for chorus:hasEndpoint ---

export async function fetchDomainServices(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?method ?routePath ?filePath WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasEndpoint ?ep . ?ep a chorus:Endpoint ; chorus:httpMethod ?method ; chorus:routePath ?routePath ; chorus:filePath ?filePath . } } ORDER BY ?method ?routePath`;
    const result = await deps.sparql(query);
    const endpoints = result.results.bindings.map((b) => ({
      method: b.method?.value || '',
      path: b.routePath?.value || '',
      handler: b.filePath?.value || '',
    }));
    const byMethod: Record<string, number> = {};
    for (const e of endpoints) byMethod[e.method] = (byMethod[e.method] || 0) + 1;
    return {
      status: 200,
      body: deps.envelope('domain-services', { subdomain: sdId, endpoints, byMethod }, now() - start, { count: endpoints.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-services', { subdomain: subdomainName, endpoints: [], byMethod: {} }, now() - start, { count: 0 }),
    };
  }
}

// --- alerts: filesystem scan of proving/domains/alerts/*.yml ---

export interface AlertFile {
  file: string;
  content: string;
}

export interface DomainAlertsDeps extends DomainFacetDeps {
  readAlertFiles: () => AlertFile[];
}

type DomainAlertEntry = { file: string; name: string; description: string; severity: string; schedule: string };

function alertMatches(file: string, content: string, tokens: string[]): boolean {
  const lower = content.toLowerCase();
  const fileLower = file.toLowerCase();
  return tokens.some((t) => lower.includes(t) || fileLower.includes(t));
}

function parseDomainAlert(file: string, content: string): DomainAlertEntry {
  return {
    file,
    name: content.match(/^name:\s*(.+)/m)?.[1]?.trim() || file.replace('.yml', ''),
    description: content.match(/^description:\s*(.+)/m)?.[1]?.trim() || '',
    severity: content.match(/^severity:\s*(.+)/m)?.[1]?.trim() || 'unknown',
    schedule: content.match(/^schedule:\s*"?(.+?)"?\s*$/m)?.[1]?.trim() || '',
  };
}

function collectDomainAlerts(deps: DomainAlertsDeps, tokens: string[]): DomainAlertEntry[] {
  const alerts: DomainAlertEntry[] = [];
  for (const { file, content } of deps.readAlertFiles()) {
    if (alertMatches(file, content, tokens)) alerts.push(parseDomainAlert(file, content));
  }
  return alerts;
}

export async function fetchDomainAlerts(
  deps: DomainAlertsDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const identity = resolveDomainIdentity(subdomainName);
    const alerts = collectDomainAlerts(deps, identity.alertFileTokens);
    return {
      status: 200,
      body: deps.envelope('domain-alerts', { subdomain: sdId, domainLabel: identity.primary, alerts }, now() - start, { count: alerts.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-alerts', { subdomain: subdomainName, alerts: [] }, now() - start, { count: 0 }),
    };
  }
}

// --- radius: outward neighborhood walk (#2028) ---

export async function fetchDomainRadius(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX borg: <urn:borg:ontology/>
SELECT ?target ?label ?relationship ?direction WHERE {
  {
    GRAPH <urn:chorus:ontology> {
      { ?parent chorus:hasDomain <${sdUri}> . ?parent rdfs:label ?label . BIND("hasDomain" AS ?relationship) BIND("parent" AS ?direction) BIND(?parent AS ?target) }
      UNION { <${sdUri}> chorus:consumes ?target . OPTIONAL { ?target rdfs:label ?label } BIND("consumes" AS ?relationship) BIND("outbound" AS ?direction) }
      UNION { ?target chorus:consumes <${sdUri}> . OPTIONAL { ?target rdfs:label ?label } BIND("consumedBy" AS ?relationship) BIND("inbound" AS ?direction) }
      UNION { ?parent chorus:contains <${sdUri}> . ?parent rdfs:label ?label . BIND("containedBy" AS ?relationship) BIND("parent" AS ?direction) BIND(?parent AS ?target) }
    }
  } UNION {
    GRAPH <urn:borg:instances> {
      { <${sdUri}> borg:usesEnvironment ?env . ?env borg:environmentName ?label . BIND("usesEnvironment" AS ?relationship) BIND("outbound" AS ?direction) BIND(?env AS ?target) }
    }
  }
}`;
    const result = await deps.sparql(query);
    const edges = result.results.bindings.map((b) => ({
      target: b.label?.value || (b.target?.value || '').split('#').pop() || '',
      targetUri: b.target?.value || '',
      relationship: b.relationship?.value || '',
      direction: b.direction?.value || '',
    }));
    return {
      status: 200,
      body: deps.envelope('domain-radius', { subdomain: sdId, edges }, now() - start, { count: edges.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-radius', { subdomain: subdomainName, edges: [] }, now() - start, { count: 0 }),
    };
  }
}

// --- blast-radius: inward impact walk (#2028) ---

export async function fetchDomainBlastRadius(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX borg: <urn:borg:ontology/>
SELECT ?target ?label ?relationship ?direction WHERE {
  {
    GRAPH <urn:chorus:ontology> {
      { ?target chorus:consumes <${sdUri}> . OPTIONAL { ?target rdfs:label ?label } BIND("consumes" AS ?relationship) BIND("dependent" AS ?direction) }
      UNION { ?target chorus:hasDomain <${sdUri}> . OPTIONAL { ?target rdfs:label ?label } BIND("ownerProduct" AS ?relationship) BIND("parent" AS ?direction) }
      UNION { ?target chorus:contains <${sdUri}> . OPTIONAL { ?target rdfs:label ?label } BIND("containedBy" AS ?relationship) BIND("parent" AS ?direction) }
    }
  } UNION {
    GRAPH <urn:borg:instances> {
      { ?otherDomain borg:usesEnvironment ?env . <${sdUri}> borg:usesEnvironment ?env . ?env borg:environmentName ?envName . FILTER(?otherDomain != <${sdUri}>) OPTIONAL { GRAPH <urn:chorus:ontology> { ?otherDomain rdfs:label ?label } } BIND("sharedInfra" AS ?relationship) BIND("co-dependent" AS ?direction) BIND(?otherDomain AS ?target) }
    }
  }
}`;
    const result = await deps.sparql(query);
    const seen = new Set<string>();
    const edges: Array<{ target: string; targetUri: string; relationship: string; direction: string }> = [];
    for (const b of result.results.bindings) {
      const targetUri = b.target?.value || '';
      const relationship = b.relationship?.value || '';
      const key = `${targetUri}|${relationship}`;
      if (!seen.has(key)) {
        seen.add(key);
        edges.push({
          target: b.label?.value || targetUri.split('#').pop() || '',
          targetUri,
          relationship,
          direction: b.direction?.value || '',
        });
      }
    }
    return {
      status: 200,
      body: deps.envelope('domain-blast-radius', { subdomain: sdId, edges }, now() - start, { count: edges.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-blast-radius', { subdomain: subdomainName, edges: [] }, now() - start, { count: 0 }),
    };
  }
}

// --- infra: borg environments via urn:borg:instances usesEnvironment edges ---

interface InfraEnv { name: string; port: string | null; health: string | null; host: string | null; engine: string | null; dependsOn: string[]; }

type InfraBinding = {
  name?: { value: string };
  port?: { value: string };
  health?: { value: string };
  host?: { value: string };
  engine?: { value: string };
  dep?: { value: string };
};

function buildInfraQuery(sdId: string): string {
  return `PREFIX borg: <urn:borg:ontology/>
PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?env ?name ?port ?health ?host ?engine ?dep WHERE {
  GRAPH <urn:borg:instances> {
    <https://jeffbridwell.com/chorus#${sdId}> borg:usesEnvironment ?env .
    ?env borg:environmentName ?name .
    OPTIONAL { ?env borg:port ?port }
    OPTIONAL { ?env borg:healthEndpoint ?health }
    OPTIONAL { ?env borg:runsOn/borg:hostName ?host }
    OPTIONAL { ?env borg:instanceOf/borg:engineName ?engine }
    OPTIONAL { ?env borg:dependsOn/borg:environmentName ?dep }
  }
} ORDER BY ?host ?name`;
}

function makeInfraEnv(envName: string, b: InfraBinding): InfraEnv {
  return {
    name: envName,
    port: b.port?.value || null,
    health: b.health?.value || null,
    host: b.host?.value || null,
    engine: b.engine?.value || null,
    dependsOn: [],
  };
}

function ensureInfraEnv(map: Map<string, InfraEnv>, b: InfraBinding): void {
  const envName = b.name?.value || '';
  if (!envName) return;
  let env = map.get(envName);
  if (!env) { env = makeInfraEnv(envName, b); map.set(envName, env); }
  const dep = b.dep?.value;
  if (dep && !env.dependsOn.includes(dep)) env.dependsOn.push(dep);
}

function foldInfraBindings(bindings: InfraBinding[]): InfraEnv[] {
  const map = new Map<string, InfraEnv>();
  for (const b of bindings) ensureInfraEnv(map, b);
  return Array.from(map.values());
}

export async function fetchDomainInfra(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const sdId = await deps.resolveSubdomainId(subdomainName);
    const result = await deps.sparql(buildInfraQuery(sdId));
    const environments = foldInfraBindings(result.results.bindings as InfraBinding[]);
    return {
      status: 200,
      body: deps.envelope('domain-infra', { subdomain: subdomainName, environments }, now() - start, { count: environments.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-infra', { subdomain: subdomainName, environments: [] }, now() - start, { count: 0 }),
    };
  }
}

// --- decisions: SPARQL against urn:chorus:decisions, via shared resolver (#2430) ---
// Pre-#2430 this handler had its own DECISION_ALIASES table in underscore-case
// (loom_principles) whose primary URI was always garbage — `loom_principles-domain`
// doesn't exist in Fuseki (which uses kebab). The alias fell through to the
// right URI (`loom-domain`) by accident. Resolver fixes this properly.

export async function fetchDomainDecisions(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const identity = resolveDomainIdentity(subdomainName);
    const domainNames = [identity.primary, ...identity.aliases];
    const domainFilter = domainNames.map((n) => `<https://jeffbridwell.com/chorus#${n}-domain>`).join(', ');
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?id ?title ?date ?status ?level ?type WHERE { GRAPH <urn:chorus:decisions> { ?s a chorus:Decision ; rdfs:label ?id ; rdfs:comment ?title ; chorus:decisionDate ?date ; chorus:decisionStatus ?status ; chorus:enforcementLevel ?level ; chorus:decisionType ?type ; chorus:hasDomain ?dom . FILTER(?dom IN (${domainFilter})) } } ORDER BY ?type ?id`;
    const result = await deps.sparql(query);
    const decisions = result.results.bindings.map((b) => ({
      id: b.id?.value || '',
      title: b.title?.value || '',
      date: b.date?.value || '',
      status: b.status?.value || '',
      enforcement: b.level?.value || '',
      type: b.type?.value || '',
    }));
    const byEnforcement: Record<string, number> = {};
    for (const d of decisions) byEnforcement[d.enforcement] = (byEnforcement[d.enforcement] || 0) + 1;
    return {
      status: 200,
      body: deps.envelope('domain-decisions', { domain: subdomainName, decisions, byEnforcement }, now() - start, { count: decisions.length }),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope('domain-decisions', { domain: subdomainName, decisions: [], byEnforcement: {} }, now() - start, { count: 0 }),
    };
  }
}
