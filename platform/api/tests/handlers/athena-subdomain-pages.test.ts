import { fetchAthenaSubdomainPages, type AthenaSubdomainPagesDeps, type SparqlPageBinding } from '../../src/handlers/athena-subdomain-pages';

const result = (bindings: SparqlPageBinding[]) => ({ results: { bindings } });
const deps = (o: Partial<AthenaSubdomainPagesDeps> = {}): AthenaSubdomainPagesDeps => ({
  sparql: async () => result([]), now: () => 1_000_000, ...o,
});

describe('fetchAthenaSubdomainPages (#2187)', () => {
  test('maps bindings to {route, path, pageType} and groups byType', async () => {
    const r = await fetchAthenaSubdomainPages(deps({
      sparql: async () => result([
        { route: { value: '/a' }, filePath: { value: 'views/a.html' }, pageType: { value: 'landing' } },
        { route: { value: '/b' }, filePath: { value: 'views/b.html' }, pageType: { value: 'landing' } },
        { route: { value: '/c' }, filePath: { value: 'views/c.html' }, pageType: { value: 'detail' } },
      ]),
    }), 'x');
    expect(r.status).toBe(200);
    const body = r.body as { data: { pages: Array<{ route: string; path: string; pageType: string }>; byType: Record<string, number> } };
    expect(body.data.pages.map((p) => p.route)).toEqual(['/a', '/b', '/c']);
    expect(body.data.byType).toEqual({ landing: 2, detail: 1 });
  });

  test('query contains built sub-domain URI', async () => {
    let q = '';
    await fetchAthenaSubdomainPages(deps({ sparql: async (qq) => { q = qq; return result([]); } }), 'chorus-api');
    expect(q).toContain('https://jeffbridwell.com/chorus#chorus-api');
  });

  test('SPARQL throws returns 500', async () => {
    const r = await fetchAthenaSubdomainPages(deps({ sparql: async () => { throw new Error('x'); } }), 'x');
    expect(r.status).toBe(500);
  });
});
