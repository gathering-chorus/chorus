/**
 * athena-owners handler — unit tests (#2187).
 *
 * Lists owners with sub-domain counts. Trivial mapping — each binding is a
 * single owner with a parsed count.
 */
import {
  fetchAthenaOwners,
  type AthenaOwnersDeps,
  type SparqlOwnerBinding,
} from '../../src/handlers/athena-owners';

function result(bindings: SparqlOwnerBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaOwnersDeps> = {}): AthenaOwnersDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaOwners (#2187)', () => {
  test('empty result returns 200 with empty array', async () => {
    const r = await fetchAthenaOwners(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('full binding maps uri, label, subdomainCount', async () => {
    const r = await fetchAthenaOwners(deps({
      sparql: async () => result([
        {
          owner: { value: 'https://jeffbridwell.com/chorus#silas' },
          label: { value: 'Silas' },
          count: { value: '7' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ uri: string; label: string; subdomainCount: number }> };
    expect(body.data[0]).toEqual({
      uri: 'https://jeffbridwell.com/chorus#silas',
      label: 'Silas',
      subdomainCount: 7,
    });
  });

  test('missing label falls back to URI fragment', async () => {
    const r = await fetchAthenaOwners(deps({
      sparql: async () => result([
        { owner: { value: 'https://jeffbridwell.com/chorus#anonymous' }, count: { value: '1' } },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string }> };
    expect(body.data[0].label).toBe('anonymous');
  });

  test('count parses as integer', async () => {
    const r = await fetchAthenaOwners(deps({
      sparql: async () => result([
        { owner: { value: '#o' }, label: { value: 'O' }, count: { value: '42' } },
      ]),
    }));
    const body = r.body as { data: Array<{ subdomainCount: number }> };
    expect(body.data[0].subdomainCount).toBe(42);
    expect(typeof body.data[0].subdomainCount).toBe('number');
  });

  test('multiple owners yield matching count', async () => {
    const r = await fetchAthenaOwners(deps({
      sparql: async () => result([
        { owner: { value: '#a' }, label: { value: 'A' }, count: { value: '1' } },
        { owner: { value: '#b' }, label: { value: 'B' }, count: { value: '2' } },
        { owner: { value: '#c' }, label: { value: 'C' }, count: { value: '3' } },
      ]),
    }));
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toHaveLength(3);
    expect(body._meta.count).toBe(3);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaOwners(deps({
      sparql: async () => { throw new Error('offline'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('offline');
    expect(body._meta.error).toBe(true);
  });

  test('loadQuery is called with name "owners"', async () => {
    let seenName = '';
    await fetchAthenaOwners(deps({
      loadQuery: (name) => { seenName = name; return '# q'; },
    }));
    expect(seenName).toBe('owners');
  });
});
