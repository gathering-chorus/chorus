/**
 * athena-blast-radius handler — unit tests (#2187).
 *
 * "What breaks if this sub-domain fails" — returns list of consumers
 * that depend on the given sub-domain. Query substitutes $URI.
 */
import {
  fetchAthenaBlastRadius,
  type AthenaBlastRadiusDeps,
  type SparqlConsumerBinding,
} from '../../src/handlers/athena-blast-radius';

function result(bindings: SparqlConsumerBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaBlastRadiusDeps> = {}): AthenaBlastRadiusDeps {
  return {
    sparql: async () => result([]),
    loadQuery: () => 'SELECT ?consumer ?consumerLabel WHERE { ?consumer chorus:consumes <$URI> }',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaBlastRadius (#2187)', () => {
  test('no consumers returns 200 with empty list and count 0', async () => {
    const r = await fetchAthenaBlastRadius(deps(), 'pulse');
    expect(r.status).toBe(200);
    const body = r.body as { data: { subdomain: string; consumers: Array<unknown> }; _meta: { count: number } };
    expect(body.data.subdomain).toBe('pulse');
    expect(body.data.consumers).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('binding maps consumer uri and label', async () => {
    const r = await fetchAthenaBlastRadius(deps({
      sparql: async () => result([{
        consumer: { value: 'https://jeffbridwell.com/chorus#chorus-api' },
        consumerLabel: { value: 'chorus-api' },
      }]),
    }), 'pulse');
    const body = r.body as { data: { consumers: Array<{ uri: string; label: string }> } };
    expect(body.data.consumers).toEqual([
      { uri: 'https://jeffbridwell.com/chorus#chorus-api', label: 'chorus-api' },
    ]);
  });

  test('missing label falls back to URI fragment', async () => {
    const r = await fetchAthenaBlastRadius(deps({
      sparql: async () => result([{ consumer: { value: 'https://jeffbridwell.com/chorus#bare' } }]),
    }), 'x');
    const body = r.body as { data: { consumers: Array<{ label: string }> } };
    expect(body.data.consumers[0].label).toBe('bare');
  });

  test('$URI placeholder is replaced with built sub-domain URI', async () => {
    let seen = '';
    await fetchAthenaBlastRadius(deps({
      loadQuery: () => 'SELECT * WHERE { <$URI> ... }',
      sparql: async (q) => { seen = q; return result([]); },
    }), 'my-domain');
    expect(seen).toContain('https://jeffbridwell.com/chorus#my-domain');
    expect(seen).not.toContain('$URI');
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaBlastRadius(deps({
      sparql: async () => { throw new Error('offline'); },
    }), 'x');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('offline');
    expect(body._meta.error).toBe(true);
  });
});
