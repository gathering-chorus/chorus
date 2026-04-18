/**
 * athena-steps handler — unit tests (#2187).
 *
 * Lists value-stream steps with the sub-domains at each step.
 * Binding groups: one step can produce multiple rows (one per sub-domain).
 *
 * Real logic exercised:
 *   - deduping rows into one record per step URI
 *   - accumulating sub-domains into the step's subdomains[]
 *   - domainCount tracks subdomains.length
 *   - step with no sub-domain (b.sd absent) still produces a step entry
 *   - label fallbacks for step, sub-domain (URI fragment)
 *   - owner defaults to null when missing
 */
import {
  fetchAthenaSteps,
  type AthenaStepsDeps,
  type SparqlStepBinding,
} from '../../src/handlers/athena-steps';

function result(bindings: SparqlStepBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaStepsDeps> = {}): AthenaStepsDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSteps (#2187)', () => {
  test('empty result returns 200 with empty array', async () => {
    const r = await fetchAthenaSteps(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('two bindings for the same step produce one step with two sub-domains', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => result([
        {
          step: { value: 'https://jeffbridwell.com/chorus#observation' },
          stepLabel: { value: 'Observation' },
          sd: { value: 'https://jeffbridwell.com/chorus#pulse' },
          sdLabel: { value: 'Pulse' },
          sdOwnerLabel: { value: 'Silas' },
        },
        {
          step: { value: 'https://jeffbridwell.com/chorus#observation' },
          stepLabel: { value: 'Observation' },
          sd: { value: 'https://jeffbridwell.com/chorus#athena' },
          sdLabel: { value: 'Athena' },
          sdOwnerLabel: { value: 'Wren' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string; domainCount: number; subdomains: Array<{ label: string; owner: string }> }> };
    expect(body.data).toHaveLength(1);
    expect(body.data[0].label).toBe('Observation');
    expect(body.data[0].domainCount).toBe(2);
    expect(body.data[0].subdomains.map((s) => s.label)).toEqual(['Pulse', 'Athena']);
    expect(body.data[0].subdomains[0].owner).toBe('Silas');
  });

  test('step with no sub-domain still produces a step entry with empty subdomains', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => result([
        {
          step: { value: 'https://jeffbridwell.com/chorus#empty-step' },
          stepLabel: { value: 'Empty' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string; domainCount: number; subdomains: Array<unknown> }> };
    expect(body.data).toHaveLength(1);
    expect(body.data[0].domainCount).toBe(0);
    expect(body.data[0].subdomains).toEqual([]);
  });

  test('missing step label falls back to URI fragment', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => result([
        { step: { value: 'https://jeffbridwell.com/chorus#bare-step' } },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string }> };
    expect(body.data[0].label).toBe('bare-step');
  });

  test('missing sub-domain owner defaults to null', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => result([
        {
          step: { value: '#s' },
          stepLabel: { value: 'S' },
          sd: { value: '#d' },
          sdLabel: { value: 'D' },
        },
      ]),
    }));
    const body = r.body as { data: Array<{ subdomains: Array<{ owner: string | null }> }> };
    expect(body.data[0].subdomains[0].owner).toBeNull();
  });

  test('two different steps produce two entries in input order', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => result([
        { step: { value: '#a' }, stepLabel: { value: 'A' } },
        { step: { value: '#b' }, stepLabel: { value: 'B' } },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string }> };
    expect(body.data.map((s) => s.label)).toEqual(['A', 'B']);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaSteps(deps({
      sparql: async () => { throw new Error('down'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('down');
    expect(body._meta.error).toBe(true);
  });

  test('loadQuery is called with name "steps"', async () => {
    let seenName = '';
    await fetchAthenaSteps(deps({
      loadQuery: (name) => { seenName = name; return '# q'; },
    }));
    expect(seenName).toBe('steps');
  });
});
