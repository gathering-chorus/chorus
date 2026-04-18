/**
 * athena-subdomains handler — unit tests (#2187).
 *
 * Lists sub-domains with owner + step. Supports optional filters on
 * owner label and step label via query string. Filters are injected into
 * the SPARQL query text via string replacement.
 */
import {
  fetchAthenaSubdomains,
  type AthenaSubdomainsDeps,
  type SparqlSubdomainBinding,
} from '../../src/handlers/athena-subdomains';

function result(bindings: SparqlSubdomainBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaSubdomainsDeps> = {}): AthenaSubdomainsDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (_name: string) => 'SELECT ?sd ?label ?ownerLabel ?stepLabel WHERE { ... } ORDER BY ?label',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubdomains (#2187)', () => {
  test('empty result returns 200 with empty array and count 0', async () => {
    const r = await fetchAthenaSubdomains(deps(), {});
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('binding maps uri, id (fragment), label, owner, step', async () => {
    const r = await fetchAthenaSubdomains(deps({
      sparql: async () => result([{
        sd: { value: 'https://jeffbridwell.com/chorus#chorus-api' },
        label: { value: 'chorus-api' },
        ownerLabel: { value: 'Silas' },
        stepLabel: { value: 'Serving' },
      }]),
    }), {});
    const body = r.body as { data: Array<{ uri: string; id: string; label: string; owner: string; step: string }> };
    expect(body.data[0]).toEqual({
      uri: 'https://jeffbridwell.com/chorus#chorus-api',
      id: 'chorus-api',
      label: 'chorus-api',
      owner: 'Silas',
      step: 'Serving',
    });
  });

  test('missing label falls back to id (URI fragment)', async () => {
    const r = await fetchAthenaSubdomains(deps({
      sparql: async () => result([{
        sd: { value: 'https://jeffbridwell.com/chorus#bare-sd' },
      }]),
    }), {});
    const body = r.body as { data: Array<{ label: string; id: string }> };
    expect(body.data[0].label).toBe('bare-sd');
    expect(body.data[0].id).toBe('bare-sd');
  });

  test('missing owner and step default to null', async () => {
    const r = await fetchAthenaSubdomains(deps({
      sparql: async () => result([{
        sd: { value: '#x' }, label: { value: 'X' },
      }]),
    }), {});
    const body = r.body as { data: Array<{ owner: string | null; step: string | null }> };
    expect(body.data[0].owner).toBeNull();
    expect(body.data[0].step).toBeNull();
  });

  test('owner filter injects FILTER(LCASE) clause before ORDER BY', async () => {
    let queryPassed = '';
    await fetchAthenaSubdomains(deps({
      sparql: async (q) => { queryPassed = q; return result([]); },
    }), { owner: 'silas' });
    expect(queryPassed).toContain('FILTER(LCASE(STR(?ownerLabel)) = "silas")');
    expect(queryPassed.indexOf('FILTER')).toBeLessThan(queryPassed.indexOf('ORDER BY'));
  });

  test('owner filter lowercases input for case-insensitive match', async () => {
    let queryPassed = '';
    await fetchAthenaSubdomains(deps({
      sparql: async (q) => { queryPassed = q; return result([]); },
    }), { owner: 'SILAS' });
    expect(queryPassed).toContain('"silas"');
  });

  test('step filter injects FILTER clause', async () => {
    let queryPassed = '';
    await fetchAthenaSubdomains(deps({
      sparql: async (q) => { queryPassed = q; return result([]); },
    }), { step: 'Building' });
    expect(queryPassed).toContain('FILTER(LCASE(STR(?stepLabel)) = "building")');
  });

  test('both filters applied when both query params set', async () => {
    let queryPassed = '';
    await fetchAthenaSubdomains(deps({
      sparql: async (q) => { queryPassed = q; return result([]); },
    }), { owner: 'Wren', step: 'Coordinating' });
    expect(queryPassed).toContain('"wren"');
    expect(queryPassed).toContain('"coordinating"');
  });

  test('no filters: query passed through untouched', async () => {
    let queryPassed = '';
    await fetchAthenaSubdomains(deps({
      sparql: async (q) => { queryPassed = q; return result([]); },
    }), {});
    expect(queryPassed).not.toContain('FILTER(LCASE');
  });

  test('response meta includes filters echo', async () => {
    const r = await fetchAthenaSubdomains(deps(), { owner: 'Kade', step: 'Building' });
    const body = r.body as { _meta: { filters: { owner: string | null; step: string | null } } };
    expect(body._meta.filters).toEqual({ owner: 'Kade', step: 'Building' });
  });

  test('response meta filters defaults to null when absent', async () => {
    const r = await fetchAthenaSubdomains(deps(), {});
    const body = r.body as { _meta: { filters: { owner: string | null; step: string | null } } };
    expect(body._meta.filters).toEqual({ owner: null, step: null });
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaSubdomains(deps({
      sparql: async () => { throw new Error('bad query'); },
    }), {});
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('bad query');
    expect(body._meta.error).toBe(true);
  });
});
