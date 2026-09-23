/**
 * @test-type: unit
 * Athena handler unit tests — #2278.
 *
 * Direct handler invocation with injected deps (no HTTP, no Fuseki).
 * Follows the athena-validate.test.ts pattern. Handler code runs in Jest's
 * process so it's instrumented for coverage.
 *
 * These run on every `npm test` — no RUN_INTEGRATION flag needed.
 */

import {
  fetchAthenaHealth,
  type AthenaHealthDeps,
} from '../../src/handlers/athena-health';
import {
  fetchAthenaOwners,
  type AthenaOwnersDeps,
} from '../../src/handlers/athena-owners';
import {
  fetchAthenaSubdomainCompleteness,
  type AthenaCompletenessDeps,
} from '../../src/handlers/athena-subdomain-completeness';

const emptySparql = async () => ({ results: { bindings: [] } });
const throwingSparql = async () => { throw new Error('Fuseki down'); };
const loadQuery = (_name: string) => 'SELECT ?x WHERE { ?x ?p ?o }';

// ── fetchAthenaHealth ──

describe('fetchAthenaHealth', () => {
  test('returns 200 with ok status and tripleCount from bindings', async () => {
    const deps: AthenaHealthDeps = {
      sparql: async () => ({ results: { bindings: [{ count: { value: '99' } }] } }),
      loadQuery,
    };
    const r = await fetchAthenaHealth(deps);
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('health');
    expect(body.data.status).toBe('ok');
    expect(body.data.tripleCount).toBe(99);
  });

  test('tripleCount is 0 when bindings are empty', async () => {
    const deps: AthenaHealthDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaHealth(deps);
    expect(r.status).toBe(200);
    expect((r.body as any).data.tripleCount).toBe(0);
  });

  test('returns 503 with error status when SPARQL throws', async () => {
    const deps: AthenaHealthDeps = { sparql: throwingSparql, loadQuery };
    const r = await fetchAthenaHealth(deps);
    expect(r.status).toBe(503);
    const body = r.body as any;
    expect(body.data.status).toBe('error');
    expect(body.data.message).toContain('Fuseki down');
    expect(body._meta.error).toBe(true);
  });

  test('queries list includes health and blast-radius, not retired products (#3603) nor the subdomains list (#4274)', async () => {
    const deps: AthenaHealthDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaHealth(deps);
    const queries: any[] = (r.body as any).data.queries;
    expect(queries.some(q => q.name === 'health')).toBe(true);
    expect(queries.some(q => q.name === 'blast-radius')).toBe(true);
    expect(queries.some(q => q.name === 'subdomains')).toBe(false); // #4274: list route retired with chorus:SubDomain
    expect(queries.some(q => q.name === 'products')).toBe(false);
    expect(queries.some(q => q.name === 'subproducts')).toBe(false);
  });
});

describe('fetchAthenaOwners', () => {
  test('returns 200 with array data and athena source', async () => {
    const deps: AthenaOwnersDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaOwners(deps);
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(Array.isArray(body.data)).toBe(true);
    expect(body._meta.source).toBe('athena');
  });
});

// ── fetchAthenaSubdomainDetail ──

describe('fetchAthenaSubdomainCompleteness', () => {
  test('returns 404 for unknown subdomain (empty bindings)', async () => {
    const deps: AthenaCompletenessDeps = { sparqlQuery: emptySparql as any };
    const r = await fetchAthenaSubdomainCompleteness(deps, 'nonexistent-xyz');
    expect(r.status).toBe(404);
  });

  test('returns 200 with sections map and percentage when subdomain found', async () => {
    const metaBinding = {
      label: { value: 'Logs' },
      ownerLabel: { value: 'Silas' },
      stepLabel: { value: 'Building' },
    };
    let callCount = 0;
    const deps: AthenaCompletenessDeps = {
      sparqlQuery: async () => {
        callCount++;
        // First call is the meta query — return a binding so subdomain exists
        if (callCount === 1) return { results: { bindings: [metaBinding] } };
        // Remaining calls are COUNT queries — return n=0
        return { results: { bindings: [{ n: { value: '0' } }] } };
      },
    };
    const r = await fetchAthenaSubdomainCompleteness(deps, 'logs-domain');
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(body._meta.query_name).toBe('subdomain-completeness');
    expect(typeof body.data.percentage).toBe('number');
    expect(body.data.percentage).toBeGreaterThanOrEqual(0);
    expect(body.data.percentage).toBeLessThanOrEqual(100);
    expect(Array.isArray(body.data.present)).toBe(true);
    expect(Array.isArray(body.data.missing)).toBe(true);
    expect(body.data.lifecycle).toBeDefined();
    expect(body.data.lifecycle.create).toBeDefined();
    expect(body.data.lifecycle.wip).toBeDefined();
    expect(body.data.lifecycle.done).toBeDefined();
  });
});
