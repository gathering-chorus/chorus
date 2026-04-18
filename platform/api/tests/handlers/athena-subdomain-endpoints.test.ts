import { fetchAthenaSubdomainEndpoints, type AthenaSubdomainEndpointsDeps, type SparqlEndpointBinding } from '../../src/handlers/athena-subdomain-endpoints';

const result = (bindings: SparqlEndpointBinding[]) => ({ results: { bindings } });
const deps = (o: Partial<AthenaSubdomainEndpointsDeps> = {}): AthenaSubdomainEndpointsDeps => ({
  sparql: async () => result([]), now: () => 1_000_000, ...o,
});

describe('fetchAthenaSubdomainEndpoints (#2187)', () => {
  test('maps bindings to {method, path, handler} and groups byMethod', async () => {
    const r = await fetchAthenaSubdomainEndpoints(deps({
      sparql: async () => result([
        { method: { value: 'GET' }, routePath: { value: '/a' }, filePath: { value: 'h/a.ts' } },
        { method: { value: 'GET' }, routePath: { value: '/b' }, filePath: { value: 'h/b.ts' } },
        { method: { value: 'POST' }, routePath: { value: '/c' }, filePath: { value: 'h/c.ts' } },
      ]),
    }), 'x');
    const body = r.body as { data: { endpoints: Array<{ method: string }>; byMethod: Record<string, number> } };
    expect(body.data.endpoints).toHaveLength(3);
    expect(body.data.byMethod).toEqual({ GET: 2, POST: 1 });
  });

  test('SPARQL throws returns 500', async () => {
    const r = await fetchAthenaSubdomainEndpoints(deps({ sparql: async () => { throw new Error('x'); } }), 'x');
    expect(r.status).toBe(500);
  });
});
