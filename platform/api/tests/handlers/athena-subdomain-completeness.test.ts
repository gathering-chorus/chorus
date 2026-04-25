import {
  fetchAthenaSubdomainCompleteness,
  type AthenaCompletenessDeps,
  type SparqlMetaBinding,
} from '../../src/handlers/athena-subdomain-completeness';

function metaResult(binding: SparqlMetaBinding | null) {
  return { results: { bindings: binding ? [binding] : [] } };
}
function countResult(n: number) {
  return { results: { bindings: [{ n: { value: String(n) } }] } };
}

function deps(meta: SparqlMetaBinding | null, counts: Record<string, number> = {}): AthenaCompletenessDeps {
  // #2485 — services slot now counts hasEndpoint (canonical from discover-endpoints).
  const predOrder = ['hasActor', 'hasScenario', 'hasContract', 'hasPriorArt', 'hasPage', 'hasIntegration', 'hasEndpoint', 'hasPersistence', 'hasPipeline', 'hasLogSource', 'hasGap'];
  return {
    sparqlQuery: async (q) => {
      if (q.includes('SubDomain')) return metaResult(meta);
      for (const pred of predOrder) {
        if (q.includes(`chorus:${pred}`)) return countResult(counts[pred] ?? 0);
      }
      return countResult(0);
    },
    now: () => 1_000_000,
  };
}

describe('fetchAthenaSubdomainCompleteness (#2187)', () => {
  test('empty meta bindings → 404', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(null), 'missing');
    expect(r.status).toBe(404);
  });

  test('all sections present → 100% and all lifecycle gates pass', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(
      {
        label: { value: 'X' }, comment: { value: 'c' },
        ownerLabel: { value: 'Silas' }, stepLabel: { value: 'Building' },
        consumesCount: { value: '2' }, consumedByCount: { value: '1' },
      },
      { hasActor: 1, hasScenario: 1, hasContract: 1, hasPriorArt: 1, hasPage: 1, hasIntegration: 1, hasEndpoint: 1, hasPersistence: 1, hasPipeline: 1, hasLogSource: 1, hasGap: 1 },
    ), 'x');
    expect(r.status).toBe(200);
    const body = r.body as { data: { percentage: number; lifecycle: Record<string, { pass: boolean }> } };
    expect(body.data.percentage).toBe(100);
    expect(body.data.lifecycle.create.pass).toBe(true);
    expect(body.data.lifecycle.wip.pass).toBe(true);
    expect(body.data.lifecycle.done.pass).toBe(true);
  });

  test('only label present → create gate fails, lists missing sections', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps({ label: { value: 'X' } }), 'x');
    const body = r.body as { data: { lifecycle: { create: { pass: boolean; missing: string[] } } } };
    expect(body.data.lifecycle.create.pass).toBe(false);
    expect(body.data.lifecycle.create.missing).toEqual(expect.arrayContaining(['owner', 'step', 'comment']));
  });

  test('#2485 wip gate requires edges + at least one required-authored facet', async () => {
    // wip.pass = edges + ANY one of {actors, scenarios, contract}. Reshaped
    // from the old "actors AND edges" rule. Designer started the design work
    // is the signal — actors OR scenarios OR contract counts as evidence.
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' } },
      { hasActor: 1 },
    ), 'x');
    const body = r.body as { data: { lifecycle: { wip: { pass: boolean; missing: string[] } } } };
    expect(body.data.lifecycle.wip.pass).toBe(false);
    expect(body.data.lifecycle.wip.missing).toContain('edges');
  });

  test('#2485 wip gate passes when edges present + scenarios authored (alternative to actors)', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' }, consumedByCount: { value: '1' } },
      { hasScenario: 1 },
    ), 'x');
    const body = r.body as { data: { lifecycle: { wip: { pass: boolean } } } };
    expect(body.data.lifecycle.wip.pass).toBe(true);
  });

  test('#2485 done gate requires all required-authored: actors + scenarios + contract', async () => {
    // done.pass = all three required-authored facets present, regardless of
    // optional facets. Reshape from old "scenarios + contract" rule.
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' }, comment: { value: 'c' }, ownerLabel: { value: 'O' }, stepLabel: { value: 'S' }, consumedByCount: { value: '1' } },
      { hasActor: 1, hasScenario: 1, hasContract: 1 },
    ), 'x');
    const body = r.body as { data: { lifecycle: { done: { pass: boolean; missing: string[] } } } };
    expect(body.data.lifecycle.done.pass).toBe(true);
    expect(body.data.lifecycle.done.missing).toEqual([]);
  });

  test('#2485 done gate fails when actors missing (was passing under old scenarios+contract rule)', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' }, comment: { value: 'c' }, ownerLabel: { value: 'O' }, stepLabel: { value: 'S' }, consumedByCount: { value: '1' } },
      { hasScenario: 1, hasContract: 1 },
    ), 'x');
    const body = r.body as { data: { lifecycle: { done: { pass: boolean; missing: string[] } } } };
    expect(body.data.lifecycle.done.pass).toBe(false);
    expect(body.data.lifecycle.done.missing).toContain('actors');
  });

  test('#2485 substrate-class subdomain (loom-*) requires prior_art for done.pass', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' }, comment: { value: 'c' }, ownerLabel: { value: 'O' }, stepLabel: { value: 'S' }, consumedByCount: { value: '1' } },
      { hasActor: 1, hasScenario: 1, hasContract: 1 },
    ), 'loom-decisions');
    const body = r.body as { data: { lifecycle: { done: { pass: boolean; missing: string[] } } } };
    expect(body.data.lifecycle.done.pass).toBe(false);
    expect(body.data.lifecycle.done.missing).toContain('prior_art');
  });

  test('edges section true when consumesCount or consumedByCount > 0', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(
      { label: { value: 'X' }, consumedByCount: { value: '3' } },
      { hasActor: 1 },
    ), 'x');
    const body = r.body as { data: { sections: { edges: boolean } } };
    expect(body.data.sections.edges).toBe(true);
  });

  test('runs 12 queries total in parallel (1 meta + 11 counts)', async () => {
    let calls = 0;
    const r = await fetchAthenaSubdomainCompleteness({
      sparqlQuery: async (q) => {
        calls++;
        if (q.includes('SubDomain')) return metaResult({ label: { value: 'X' } });
        return countResult(0);
      },
    }, 'x');
    expect(r.status).toBe(200);
    expect(calls).toBe(12);
  });

  test('SPARQL throws → 500', async () => {
    const r = await fetchAthenaSubdomainCompleteness({
      sparqlQuery: async () => { throw new Error('fuseki down'); },
    }, 'x');
    expect(r.status).toBe(500);
  });
});
