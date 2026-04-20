/**
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
  fetchAthenaSubdomains,
  type AthenaSubdomainsDeps,
} from '../../src/handlers/athena-subdomains';
import {
  fetchAthenaProducts,
  type AthenaProductsDeps,
} from '../../src/handlers/athena-products';
import {
  fetchAthenaSteps,
  type AthenaStepsDeps,
} from '../../src/handlers/athena-steps';
import {
  fetchAthenaOwners,
  type AthenaOwnersDeps,
} from '../../src/handlers/athena-owners';
import {
  fetchAthenaSubdomainDetail,
  type AthenaSubdomainDetailDeps,
} from '../../src/handlers/athena-subdomain-detail';
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

  test('queries list includes health, products, subdomains', async () => {
    const deps: AthenaHealthDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaHealth(deps);
    const queries: any[] = (r.body as any).data.queries;
    expect(queries.some(q => q.name === 'health')).toBe(true);
    expect(queries.some(q => q.name === 'products')).toBe(true);
    expect(queries.some(q => q.name === 'subdomains')).toBe(true);
  });
});

// ── fetchAthenaSubdomains ──

describe('fetchAthenaSubdomains', () => {
  const baseDeps: AthenaSubdomainsDeps = { sparql: emptySparql, loadQuery };

  test('returns 200 with empty data array when no bindings', async () => {
    const r = await fetchAthenaSubdomains(baseDeps, {});
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(Array.isArray(body.data)).toBe(true);
    expect(body._meta.source).toBe('athena');
    expect(body._meta.count).toBe(0);
  });

  test('maps bindings to subdomain objects', async () => {
    const deps: AthenaSubdomainsDeps = {
      sparql: async () => ({
        results: {
          bindings: [{
            sd: { value: 'urn:gathering:domain/cards-service' },
            label: { value: 'Cards Service' },
            ownerLabel: { value: 'Wren' },
            stepLabel: { value: 'Directing' },
          }],
        },
      }),
      loadQuery,
    };
    const r = await fetchAthenaSubdomains(deps, {});
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(body.data.length).toBe(1);
    expect(body.data[0].label).toBe('Cards Service');
    expect(body.data[0].owner).toBe('Wren');
    expect(body.data[0].step).toBe('Directing');
    expect(body._meta.count).toBe(1);
  });

  test('returns 500 envelope when SPARQL throws', async () => {
    const deps: AthenaSubdomainsDeps = { sparql: throwingSparql, loadQuery };
    const r = await fetchAthenaSubdomains(deps, {});
    expect(r.status).toBe(500);
    expect((r.body as any)._meta.error).toBe(true);
  });
});

// ── fetchAthenaProducts ──

describe('fetchAthenaProducts', () => {
  test('returns 200 with array data and athena envelope', async () => {
    const deps: AthenaProductsDeps = { sparql: emptySparql, loadQuery, envelope: undefined };
    const r = await fetchAthenaProducts(deps);
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(Array.isArray(body.data)).toBe(true);
    expect(body._meta.source).toBe('athena');
    expect(body._meta.query_name).toBe('products');
  });

  test('returns 500 when SPARQL throws', async () => {
    const deps: AthenaProductsDeps = { sparql: throwingSparql, loadQuery, envelope: undefined };
    const r = await fetchAthenaProducts(deps);
    expect(r.status).toBe(500);
  });
});

// ── fetchAthenaSteps ──

describe('fetchAthenaSteps', () => {
  test('returns 200 with array data', async () => {
    const deps: AthenaStepsDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaSteps(deps);
    expect(r.status).toBe(200);
    expect(Array.isArray((r.body as any).data)).toBe(true);
  });

  test('returns 500 when SPARQL throws', async () => {
    const deps: AthenaStepsDeps = { sparql: throwingSparql, loadQuery };
    const r = await fetchAthenaSteps(deps);
    expect(r.status).toBe(500);
  });
});

// ── fetchAthenaOwners ──

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

describe('fetchAthenaSubdomainDetail', () => {
  function baseDeps(overrides: Partial<AthenaSubdomainDetailDeps> = {}): AthenaSubdomainDetailDeps {
    return { sparql: emptySparql as any, loadQuery, ...overrides };
  }

  test('returns 404 when subdomain not found (empty bindings)', async () => {
    const r = await fetchAthenaSubdomainDetail(baseDeps(), 'nonexistent');
    expect(r.status).toBe(404);
    const body = r.body as any;
    expect(body.data.error).toContain('not found');
    expect(body.data.suggestion).toBeDefined();
  });

  test('returns 200 with mapped data when binding present', async () => {
    const deps = baseDeps({
      sparql: async () => ({
        results: {
          bindings: [{
            sd: { value: 'urn:gathering:domain/cards-service' },
            label: { value: 'Cards Service' },
            ownerLabel: { value: 'Wren' },
            stepLabel: { value: 'Directing' },
          }],
        },
      }),
    });
    const r = await fetchAthenaSubdomainDetail(deps, 'cards-service');
    expect(r.status).toBe(200);
    const body = r.body as any;
    expect(body._meta.query_name).toBe('subdomain-detail');
    expect(body.data.label).toBe('Cards Service');
    expect(body.data.owner).toBe('Wren');
  });
});

// ── fetchAthenaSubdomainCompleteness ──

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
