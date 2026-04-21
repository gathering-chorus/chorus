/**
 * loom-policies handler — unit tests (#2339).
 */
import {
  fetchLoomPolicies,
  type LoomPoliciesDeps,
  type SparqlPolicyBinding,
} from '../../src/handlers/loom-policies';

function result(bindings: SparqlPolicyBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<LoomPoliciesDeps> = {}): LoomPoliciesDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (_name: string) => 'SELECT ...',
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchLoomPolicies (#2339)', () => {
  test('empty bindings returns 200 with empty policies array', async () => {
    const r = await fetchLoomPolicies(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: { policies: unknown[] }; _meta: { orphan_count: number } };
    expect(body.data.policies).toEqual([]);
    expect(body._meta.orphan_count).toBe(0);
  });

  test('returns policy with enforces[] array of principles', async () => {
    const r = await fetchLoomPolicies(deps({
      sparql: async () => result([
        {
          policy: { value: 'chorus#policy-demo-gate' },
          label: { value: 'Demo gate' },
          comment: { value: 'No Done without demo.' },
          surface: { value: 'hook' },
          principle: { value: 'chorus#principle-obtain-a-yield' },
          principleLabel: { value: 'Obtain a yield' },
        },
        {
          policy: { value: 'chorus#policy-demo-gate' },
          label: { value: 'Demo gate' },
          principle: { value: 'chorus#principle-quality-at-source' },
          principleLabel: { value: 'Quality at the source' },
        },
      ]),
    }));
    const body = r.body as { data: { policies: Array<{ id: string; enforces: Array<{ label: string }> }> } };
    expect(body.data.policies).toHaveLength(1);
    expect(body.data.policies[0].id).toBe('policy-demo-gate');
    expect(body.data.policies[0].enforces.map((p) => p.label)).toEqual([
      'Obtain a yield',
      'Quality at the source',
    ]);
  });

  test('orphan policy (no enforces edge) renders with empty enforces[] and counted', async () => {
    const r = await fetchLoomPolicies(deps({
      sparql: async () => result([
        {
          policy: { value: 'chorus#policy-mystery' },
          label: { value: 'Unknown policy' },
        },
      ]),
    }));
    const body = r.body as { data: { policies: Array<{ enforces: unknown[] }> }; _meta: { orphan_count: number } };
    expect(body.data.policies[0].enforces).toEqual([]);
    expect(body._meta.orphan_count).toBe(1);
  });

  test('sorts policies alphabetically by label', async () => {
    const r = await fetchLoomPolicies(deps({
      sparql: async () => result([
        { policy: { value: 'chorus#z' }, label: { value: 'Zeta policy' } },
        { policy: { value: 'chorus#a' }, label: { value: 'Alpha policy' } },
      ]),
    }));
    const body = r.body as { data: { policies: Array<{ label: string }> } };
    expect(body.data.policies.map((p) => p.label)).toEqual(['Alpha policy', 'Zeta policy']);
  });

  test('envelope includes source=loom and count', async () => {
    const r = await fetchLoomPolicies(deps({
      sparql: async () => result([
        { policy: { value: 'chorus#p' }, label: { value: 'P' } },
      ]),
    }));
    const body = r.body as { _meta: { source: string; query_name: string; count: number } };
    expect(body._meta.source).toBe('loom');
    expect(body._meta.query_name).toBe('policies');
    expect(body._meta.count).toBe(1);
  });

  test('sparql throw returns 500 envelope', async () => {
    const r = await fetchLoomPolicies(deps({
      sparql: async () => { throw new Error('sparql fail'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toContain('sparql fail');
    expect(body._meta.error).toBe(true);
  });
});
