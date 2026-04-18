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
    const domain = subdomainName.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
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

// --- decisions: SPARQL against urn:chorus:decisions with alias mapping ---

const DECISION_ALIASES: Record<string, string[]> = {
  tests: ['quality'],
  code: ['code'],
  gates: ['gates'],
};

export async function fetchDomainDecisions(
  deps: DomainFacetDeps,
  subdomainName: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  try {
    const domainName = subdomainName.replace(/-(domain|service|analytics)$/, '').replace(/-/g, '_');
    const domainNames = [domainName, ...(DECISION_ALIASES[domainName] || [])];
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
