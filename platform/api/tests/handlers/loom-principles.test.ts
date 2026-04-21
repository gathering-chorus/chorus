/**
 * loom-principles handler — unit tests (#2337).
 *
 * GET /api/loom/principles — returns all chorus:Principle instances in the
 * loom-principles subdomain, sorted by rdfs:label, as a thin product-facing
 * alias over the existing athena-subdomain-detail machinery. Empty result →
 * still returns 200 with an empty array (principles can be added any time,
 * this is not "missing resource" shaped).
 */
import {
  fetchLoomPrinciples,
  type LoomPrinciplesDeps,
  type SparqlPrincipleBinding,
} from '../../src/handlers/loom-principles';

function result(bindings: SparqlPrincipleBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<LoomPrinciplesDeps> = {}): LoomPrinciplesDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (_name: string) => 'SELECT ... WHERE { ?p a chorus:Principle }',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchLoomPrinciples (#2337)', () => {
  test('empty bindings returns 200 with empty principles array', async () => {
    const r = await fetchLoomPrinciples(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: { principles: unknown[] } };
    expect(body.data.principles).toEqual([]);
  });

  test('returns all principles with label, comment, uri, id', async () => {
    const r = await fetchLoomPrinciples(deps({
      sparql: async () => result([
        {
          principle: { value: 'https://jeffbridwell.com/chorus#principle-no-dark-work' },
          label: { value: 'No dark work' },
          comment: { value: 'Everything must be visible — board, domain graph, API.' },
        },
        {
          principle: { value: 'https://jeffbridwell.com/chorus#principle-ship-small' },
          label: { value: 'Ship small, learn fast' },
          comment: { value: 'Small cards, fast cycles, real demos.' },
        },
      ]),
    }));
    expect(r.status).toBe(200);
    const body = r.body as { data: { principles: Array<{ id: string; label: string; comment: string; uri: string }> } };
    expect(body.data.principles).toHaveLength(2);
    expect(body.data.principles[0]).toMatchObject({
      id: 'principle-no-dark-work',
      label: 'No dark work',
      comment: 'Everything must be visible — board, domain graph, API.',
      uri: 'https://jeffbridwell.com/chorus#principle-no-dark-work',
    });
  });

  test('sorts principles alphabetically by label', async () => {
    const r = await fetchLoomPrinciples(deps({
      sparql: async () => result([
        { principle: { value: 'chorus#principle-z' }, label: { value: 'Zebra principle' }, comment: { value: 'z' } },
        { principle: { value: 'chorus#principle-a' }, label: { value: 'Alpha principle' }, comment: { value: 'a' } },
        { principle: { value: 'chorus#principle-m' }, label: { value: 'Middle principle' }, comment: { value: 'm' } },
      ]),
    }));
    const body = r.body as { data: { principles: Array<{ label: string }> } };
    expect(body.data.principles.map((p) => p.label)).toEqual([
      'Alpha principle',
      'Middle principle',
      'Zebra principle',
    ]);
  });

  test('envelope includes source=loom, query_name, duration_ms, count', async () => {
    const r = await fetchLoomPrinciples(deps({
      sparql: async () => result([
        { principle: { value: 'chorus#p1' }, label: { value: 'P1' }, comment: { value: 'c1' } },
      ]),
    }));
    const body = r.body as { _meta: { source: string; query_name: string; duration_ms: number; count: number } };
    expect(body._meta.source).toBe('loom');
    expect(body._meta.query_name).toBe('principles');
    expect(typeof body._meta.duration_ms).toBe('number');
    expect(body._meta.count).toBe(1);
  });

  test('missing comment falls back to empty string, not undefined', async () => {
    const r = await fetchLoomPrinciples(deps({
      sparql: async () => result([
        { principle: { value: 'chorus#p' }, label: { value: 'No comment' } },
      ]),
    }));
    const body = r.body as { data: { principles: Array<{ comment: string }> } };
    expect(body.data.principles[0].comment).toBe('');
  });

  test('sparql throw returns 500 envelope', async () => {
    const r = await fetchLoomPrinciples(deps({
      sparql: async () => { throw new Error('Fuseki unreachable'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toContain('Fuseki unreachable');
    expect(body._meta.error).toBe(true);
  });
});
