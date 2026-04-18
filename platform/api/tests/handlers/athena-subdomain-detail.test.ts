/**
 * athena-subdomain-detail handler — unit tests (#2187).
 *
 * Returns full detail for one sub-domain: owner, step, comment, consumers
 * (things that consume it), consumes (things it depends on), child domains,
 * and contained instances. Empty bindings → 404 with suggestion.
 */
import {
  fetchAthenaSubdomainDetail,
  type AthenaSubdomainDetailDeps,
  type SparqlDetailBinding,
} from '../../src/handlers/athena-subdomain-detail';

function result(bindings: SparqlDetailBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaSubdomainDetailDeps> = {}): AthenaSubdomainDetailDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (_name: string) => 'SELECT ... WHERE { ?sd a chorus:SubDomain . FILTER(?sd = <$URI>) }',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubdomainDetail (#2187)', () => {
  test('empty bindings returns 404 with suggestion pointing to list endpoint', async () => {
    const r = await fetchAthenaSubdomainDetail(deps(), 'nonexistent');
    expect(r.status).toBe(404);
    const body = r.body as { data: { error: string; suggestion: string }; _meta: { error: boolean } };
    expect(body.data.error).toContain('nonexistent');
    expect(body.data.suggestion).toContain('/api/athena/subdomains');
    expect(body._meta.error).toBe(true);
  });

  test('single binding without relationships returns 200 with empty arrays', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([{
        label: { value: 'Pulse' },
        ownerLabel: { value: 'Silas' },
        stepLabel: { value: 'Observation' },
        comment: { value: 'Per-prompt envelope' },
      }]),
    }), 'pulse');
    expect(r.status).toBe(200);
    const body = r.body as {
      data: {
        uri: string; id: string; label: string; owner: string; step: string; comment: string;
        consumedBy: Array<unknown>; consumes: Array<unknown>; domains: Array<unknown>; instances: Array<unknown>;
      };
    };
    expect(body.data.uri).toBe('https://jeffbridwell.com/chorus#pulse');
    expect(body.data.id).toBe('pulse');
    expect(body.data.label).toBe('Pulse');
    expect(body.data.owner).toBe('Silas');
    expect(body.data.step).toBe('Observation');
    expect(body.data.comment).toBe('Per-prompt envelope');
    expect(body.data.consumedBy).toEqual([]);
    expect(body.data.consumes).toEqual([]);
    expect(body.data.domains).toEqual([]);
    expect(body.data.instances).toEqual([]);
  });

  test('duplicate consumer URIs across bindings dedupe to single consumer entry', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([
        {
          label: { value: 'Pulse' },
          consumer: { value: 'https://jeffbridwell.com/chorus#chorus-api' },
          consumerLabel: { value: 'chorus-api' },
        },
        {
          label: { value: 'Pulse' },
          consumer: { value: 'https://jeffbridwell.com/chorus#chorus-api' },
          consumerLabel: { value: 'chorus-api' },
        },
      ]),
    }), 'pulse');
    const body = r.body as { data: { consumedBy: Array<{ uri: string; label: string }> } };
    expect(body.data.consumedBy).toHaveLength(1);
    expect(body.data.consumedBy[0].uri).toBe('https://jeffbridwell.com/chorus#chorus-api');
  });

  test('distinct consumer and consumed entries populate both lists', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([
        {
          label: { value: 'Pulse' },
          consumer: { value: '#chorus-api' }, consumerLabel: { value: 'chorus-api' },
          consumed: { value: '#observer' }, consumedLabel: { value: 'observer' },
        },
      ]),
    }), 'pulse');
    const body = r.body as { data: { consumedBy: Array<{ label: string }>; consumes: Array<{ label: string }> } };
    expect(body.data.consumedBy.map((c) => c.label)).toEqual(['chorus-api']);
    expect(body.data.consumes.map((c) => c.label)).toEqual(['observer']);
  });

  test('instance accumulation dedupes by URI and maps type label', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([
        {
          label: { value: 'Pulse' },
          instance: { value: 'https://jeffbridwell.com/chorus#pulse-script' },
          instanceLabel: { value: 'pulse-script' },
          instanceComment: { value: 'observer binary' },
          instanceTypeLabel: { value: 'Script' },
        },
        {
          label: { value: 'Pulse' },
          instance: { value: 'https://jeffbridwell.com/chorus#pulse-script' },
          instanceLabel: { value: 'pulse-script' },
        },
      ]),
    }), 'pulse');
    const body = r.body as { data: { instances: Array<{ id: string; label: string; comment: string | null; type: string | null }> } };
    expect(body.data.instances).toHaveLength(1);
    expect(body.data.instances[0].id).toBe('pulse-script');
    expect(body.data.instances[0].type).toBe('Script');
    expect(body.data.instances[0].comment).toBe('observer binary');
  });

  test('instance without label or type defaults to URI fragment and null', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([{
        label: { value: 'X' },
        instance: { value: 'https://jeffbridwell.com/chorus#bare-inst' },
      }]),
    }), 'x');
    const body = r.body as { data: { instances: Array<{ label: string; comment: string | null; type: string | null }> } };
    expect(body.data.instances[0].label).toBe('bare-inst');
    expect(body.data.instances[0].comment).toBeNull();
    expect(body.data.instances[0].type).toBeNull();
  });

  test('child domains dedupe by URI', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([
        { label: { value: 'X' }, child: { value: '#c1' }, childLabel: { value: 'C1' } },
        { label: { value: 'X' }, child: { value: '#c1' }, childLabel: { value: 'C1' } },
      ]),
    }), 'x');
    const body = r.body as { data: { domains: Array<{ id: string }> } };
    expect(body.data.domains).toHaveLength(1);
  });

  test('missing label falls back to id (sub-domain URI fragment)', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([{}]),
    }), 'no-label');
    const body = r.body as { data: { label: string } };
    expect(body.data.label).toBe('no-label');
  });

  test('missing owner/step/comment default to null', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => result([{ label: { value: 'X' } }]),
    }), 'x');
    const body = r.body as { data: { owner: string | null; step: string | null; comment: string | null } };
    expect(body.data.owner).toBeNull();
    expect(body.data.step).toBeNull();
    expect(body.data.comment).toBeNull();
  });

  test('$URI placeholder in query is replaced with built sub-domain URI', async () => {
    let seenQuery = '';
    await fetchAthenaSubdomainDetail(deps({
      loadQuery: () => 'SELECT * WHERE { ?x = <$URI> }',
      sparql: async (q) => { seenQuery = q; return result([]); },
    }), 'my-id');
    expect(seenQuery).toContain('https://jeffbridwell.com/chorus#my-id');
    expect(seenQuery).not.toContain('$URI');
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaSubdomainDetail(deps({
      sparql: async () => { throw new Error('fuseki bad'); },
    }), 'x');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('fuseki bad');
    expect(body._meta.error).toBe(true);
  });
});
