// @test-type: unit — handler tests with mocked sparql.
/**
 * athena-subdomain-coverage + test-coverage handlers — unit tests (#2187).
 *
 * Two endpoints share the same SPARQL; they differ only in response shape.
 */
import {
  fetchAthenaSubdomainCoverage,
  fetchAthenaSubdomainTestCoverage,
  type AthenaSubdomainCoverageDeps,
  type SparqlTestCoverageBinding,
} from '../../src/handlers/athena-subdomain-coverage';

function result(bindings: SparqlTestCoverageBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaSubdomainCoverageDeps> = {}): AthenaSubdomainCoverageDeps {
  return {
    sparql: async () => result([]),
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubdomainCoverage (#2187)', () => {
  test('maps bindings to {testFile, testType, coversDomain=id}', async () => {
    const r = await fetchAthenaSubdomainCoverage(deps({
      sparql: async () => result([
        { testFile: { value: 'tests/a.test.ts' }, testType: { value: 'unit' } },
        { testFile: { value: 'tests/b.test.ts' }, testType: { value: 'integration' } },
      ]),
    }), 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { coverage: Array<{ testFile: string; testType: string; coversDomain: string }> } };
    expect(body.data.coverage).toEqual([
      { testFile: 'tests/a.test.ts', testType: 'unit', coversDomain: 'chorus-domain' },
      { testFile: 'tests/b.test.ts', testType: 'integration', coversDomain: 'chorus-domain' },
    ]);
  });

  test('query contains the built sub-domain URI', async () => {
    let q = '';
    await fetchAthenaSubdomainCoverage(deps({ sparql: async (qq) => { q = qq; return result([]); } }), 'pulse');
    expect(q).toContain('https://jeffbridwell.com/chorus#pulse');
  });

  test('#3442: query reads testType via hasProperty→Property, NOT a bare literal', async () => {
    // Regression guard: these mocks return a fixed binding, so a revert to the
    // bare `chorus:testType ?testType` literal would still pass the mapping
    // tests above and silently go empty against the promoted graph. Assert the
    // query SHAPE so that revert goes red here.
    let q = '';
    await fetchAthenaSubdomainCoverage(deps({ sparql: async (qq) => { q = qq; return result([]); } }), 'pulse');
    expect(q).toContain('chorus:hasProperty');
    expect(q).toContain('chorus:propertyKey "testType"');
    expect(q).toContain('chorus:propertyValue ?testType');
    expect(q).not.toMatch(/chorus:testType\s+\?testType/);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaSubdomainCoverage(deps({ sparql: async () => { throw new Error('x'); } }), 'x');
    expect(r.status).toBe(500);
  });
});

describe('fetchAthenaSubdomainTestCoverage (#2187)', () => {
  test('reshapes bindings to {path, type} and groups byType', async () => {
    const r = await fetchAthenaSubdomainTestCoverage(deps({
      sparql: async () => result([
        { testFile: { value: 'a.test.ts' }, testType: { value: 'unit' } },
        { testFile: { value: 'b.test.ts' }, testType: { value: 'unit' } },
        { testFile: { value: 'c.test.ts' }, testType: { value: 'integration' } },
      ]),
    }), 'x');
    const body = r.body as { data: { tests: Array<{ path: string; type: string }>; byType: Record<string, number> } };
    expect(body.data.tests).toEqual([
      { path: 'a.test.ts', type: 'unit' },
      { path: 'b.test.ts', type: 'unit' },
      { path: 'c.test.ts', type: 'integration' },
    ]);
    expect(body.data.byType).toEqual({ unit: 2, integration: 1 });
  });

  test('empty bindings produces empty byType and zero count', async () => {
    const r = await fetchAthenaSubdomainTestCoverage(deps(), 'x');
    const body = r.body as { data: { tests: Array<unknown>; byType: Record<string, number> }; _meta: { count: number } };
    expect(body.data.tests).toEqual([]);
    expect(body.data.byType).toEqual({});
    expect(body._meta.count).toBe(0);
  });

  test('SPARQL throws returns 500', async () => {
    const r = await fetchAthenaSubdomainTestCoverage(deps({ sparql: async () => { throw new Error('x'); } }), 'x');
    expect(r.status).toBe(500);
  });
});
