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

  test('queries list includes health, not retired products (#3603) nor any Domain route (#4274, #4353)', async () => {
    const deps: AthenaHealthDeps = { sparql: emptySparql, loadQuery };
    const r = await fetchAthenaHealth(deps);
    const queries: any[] = (r.body as any).data.queries;
    expect(queries.some(q => q.name === 'health')).toBe(true);
    expect(queries.some(q => String(q.path).includes('/domains'))).toBe(false); // #4353
    expect(queries.some(q => q.name === 'domains')).toBe(false); // #4274: list route retired with chorus:Domain
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
