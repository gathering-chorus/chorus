// @test-type: unit — fake sparql/loadQuery deps; no Fuseki, no live services.
//
// #3606 — loom-decisions.ts was 0% covered (52 statements). DI'd by design;
// these tests pin the fold (multi-domain aggregation), the ADR-first sort,
// prefix stripping, and the 500 envelope path.
import { fetchLoomDecisions, type LoomDecisionsDeps, type SparqlDecisionsResult } from '../../src/handlers/loom-decisions';

const P = 'https://jeffbridwell.com/chorus#';

function deps(result: SparqlDecisionsResult | Error): LoomDecisionsDeps & { queries: string[] } {
  const d = {
    queries: [] as string[],
    sparql: async (_q: string) => {
      d.queries.push(_q);
      if (result instanceof Error) throw result;
      return result;
    },
    loadQuery: (name: string) => `QUERY:${name}`,
    now: () => 1000,
  };
  return d;
}

function binding(uri: string, over: Record<string, string> = {}) {
  const b: Record<string, { value: string }> = { decision: { value: uri } };
  for (const [k, v] of Object.entries(over)) b[k] = { value: v };
  return b;
}

describe('fetchLoomDecisions', () => {
  it('folds one row per decision URI, aggregating hasDomain edges', async () => {
    const d = deps({
      results: {
        bindings: [
          binding(`${P}dec-100`, { id: 'DEC-100', decisionType: 'DEC', domain: `${P}photos-domain` }),
          binding(`${P}dec-100`, { id: 'DEC-100', decisionType: 'DEC', domain: `${P}music-domain` }),
          binding(`${P}dec-100`, { id: 'DEC-100', decisionType: 'DEC', domain: `${P}photos-domain` }), // dup edge
        ],
      },
    });
    const r = await fetchLoomDecisions(d);
    expect(r.status).toBe(200);
    const { decisions } = (r.body as { data: { decisions: Array<{ uri: string; domains: string[] }> } }).data;
    expect(decisions).toHaveLength(1);
    expect(decisions[0].domains).toEqual(['photos-domain', 'music-domain']);
    expect(d.queries).toEqual(['QUERY:loom-decisions']);
  });

  it('sorts ADRs before DEC/protocol, then id descending within type', async () => {
    const d = deps({
      results: {
        bindings: [
          binding(`${P}dec-050`, { id: 'DEC-050', decisionType: 'DEC' }),
          binding(`${P}adr-002`, { id: 'ADR-002', decisionType: 'ADR' }),
          binding(`${P}dec-090`, { id: 'DEC-090', decisionType: 'protocol' }),
          binding(`${P}adr-010`, { id: 'ADR-010', decisionType: 'ADR' }),
        ],
      },
    });
    const r = await fetchLoomDecisions(d);
    const ids = (r.body as { data: { decisions: Array<{ id: string }> } }).data.decisions.map((x) => x.id);
    expect(ids).toEqual(['ADR-010', 'ADR-002', 'DEC-090', 'DEC-050']);
  });

  it('strips non-chorus prefixes from domain URIs (hash first, then colon)', async () => {
    const d = deps({
      results: {
        bindings: [
          binding(`${P}dec-1`, { id: 'DEC-1', domain: 'urn:gathering:photos' }),
          binding(`${P}dec-1`, { id: 'DEC-1', domain: 'http://x.org/onto#garden' }),
          binding(`${P}dec-1`, { id: 'DEC-1', domain: `${P}werk-domain` }),
        ],
      },
    });
    const r = await fetchLoomDecisions(d);
    const { decisions } = (r.body as { data: { decisions: Array<{ domains: string[] }> } }).data;
    expect(decisions[0].domains).toEqual(['photos', 'garden', 'werk-domain']);
  });

  it('rows without a decision URI are dropped; empty results → empty list with count', async () => {
    const d = deps({ results: { bindings: [{ id: { value: 'DEC-9' } }] } });
    const r = await fetchLoomDecisions(d);
    const body = r.body as { _meta: { count: number }; data: { decisions: unknown[] } };
    expect(body.data.decisions).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  it('sparql failure returns a 500 error envelope, not a throw', async () => {
    const d = deps(new Error('fuseki down'));
    const r = await fetchLoomDecisions(d);
    expect(r.status).toBe(500);
    const body = r.body as { _meta: { error?: boolean }; data: { error: string } };
    expect(body.data.error).toBe('fuseki down');
    expect(body._meta.error).toBe(true);
  });
});
