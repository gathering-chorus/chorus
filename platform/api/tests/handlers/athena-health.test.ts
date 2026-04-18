/**
 * athena-health handler — unit tests (#2173 AC4).
 *
 * Second handler extraction, Fuseki-backed. Tests describe Jeff-visible
 * behavior of the health endpoint:
 *   - ontology up + 1383 triples → 200 + status:ok + tripleCount
 *   - ontology down (SPARQL throws) → 503 + error envelope
 *   - missing count binding → defaults to 0 (not NaN, not crash)
 *   - envelope preserves the duration + query_name metadata
 *
 * No SPARQL client, no Fuseki, no harness. Fakes pass canned results.
 */

import { fetchAthenaHealth, type AthenaHealthDeps, type SparqlResult } from '../../src/handlers/athena-health';

function okResult(count: number): SparqlResult {
  return { results: { bindings: [{ count: { value: String(count) } }] } };
}

function deps(overrides: Partial<AthenaHealthDeps> = {}): AthenaHealthDeps {
  return {
    sparql: async () => okResult(1000),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaHealth (#2173 AC4)', () => {
  test('ontology up returns 200 with tripleCount', async () => {
    const r = await fetchAthenaHealth(deps({ sparql: async () => okResult(1383) }));
    expect(r.status).toBe(200);
    const body = r.body as { data: { status: string; tripleCount: number } };
    expect(body.data.status).toBe('ok');
    expect(body.data.tripleCount).toBe(1383);
  });

  test('missing count binding defaults to 0', async () => {
    const empty: SparqlResult = { results: { bindings: [] } };
    const r = await fetchAthenaHealth(deps({ sparql: async () => empty }));
    expect(r.status).toBe(200);
    const body = r.body as { data: { tripleCount: number } };
    expect(body.data.tripleCount).toBe(0);
  });

  test('binding with missing count value defaults to 0', async () => {
    const partial: SparqlResult = { results: { bindings: [{ something: { value: 'x' } }] } };
    const r = await fetchAthenaHealth(deps({ sparql: async () => partial }));
    expect(r.status).toBe(200);
    const body = r.body as { data: { tripleCount: number } };
    expect(body.data.tripleCount).toBe(0);
  });

  test('SPARQL throws maps to 503 with error envelope', async () => {
    const r = await fetchAthenaHealth(deps({
      sparql: async () => { throw new Error('Fuseki 500: connection refused'); },
    }));
    expect(r.status).toBe(503);
    const body = r.body as { data: { status: string; message: string } };
    expect(body.data.status).toBe('error');
    expect(body.data.message).toBe('Fuseki 500: connection refused');
  });

  test('non-Error throw stringifies in error message', async () => {
    const r = await fetchAthenaHealth(deps({
      sparql: async () => { throw 'timeout'; },
    }));
    expect(r.status).toBe(503);
    const body = r.body as { data: { message: string } };
    expect(body.data.message).toBe('timeout');
  });

  test('envelope records duration_ms from now() delta', async () => {
    let callCount = 0;
    const r = await fetchAthenaHealth(deps({
      now: () => { callCount++; return callCount === 1 ? 1000 : 1042; },
    }));
    const body = r.body as { _meta: { duration_ms: number } };
    expect(body._meta.duration_ms).toBe(42);
  });

  test('passes the loaded query string to sparql()', async () => {
    let seenQuery = '';
    await fetchAthenaHealth(deps({
      loadQuery: () => 'SELECT (COUNT(*) AS ?count) WHERE { ?s ?p ?o }',
      sparql: async (q) => { seenQuery = q; return okResult(5); },
    }));
    expect(seenQuery).toBe('SELECT (COUNT(*) AS ?count) WHERE { ?s ?p ?o }');
  });

  test('loadQuery is called with the name "health"', async () => {
    let seenName = '';
    await fetchAthenaHealth(deps({
      loadQuery: (name) => { seenName = name; return `# ${name}`; },
    }));
    expect(seenName).toBe('health');
  });

  test('envelope marks error=true on failure path', async () => {
    const r = await fetchAthenaHealth(deps({
      sparql: async () => { throw new Error('boom'); },
    }));
    const body = r.body as { _meta: { error?: boolean } };
    expect(body._meta.error).toBe(true);
  });

  test('success envelope includes queries list', async () => {
    const r = await fetchAthenaHealth(deps());
    const body = r.body as { data: { queries: Array<{ name: string }> } };
    expect(Array.isArray(body.data.queries)).toBe(true);
    expect(body.data.queries.length).toBeGreaterThan(0);
    expect(body.data.queries.find((q) => q.name === 'health')).toBeDefined();
  });
});
