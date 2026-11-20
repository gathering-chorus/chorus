// @test-type: unit — signal:integration is fixture-data (handler tests with mocked fetch/sparql).
// @domain: logs — placed by chorus-crawl's domain rules (#4538)
/**
 * domain-facets handlers — unit tests (#2173 AC4).
 *
 * Four Fuseki-backed facet handlers share DomainFacetDeps. Tests prove
 * Jeff-visible behavior: empty SPARQL result → empty envelope + count 0;
 * populated result → shaped entities; byType/byMethod/byEnforcement
 * aggregation; error fallback returns 200 with empty envelope (not 500);
 * upstream fetch failure on tests handler same shape.
 */

import {
  fetchDomainTests,
  fetchDomainServices,
  fetchDomainDecisions,
  fetchDomainRadius,
  fetchDomainBlastRadius,
  fetchDomainAlerts,
  fetchDomainInfra,
  type DomainFacetDeps,
} from '../../src/handlers/domain-facets';
import type { SparqlResult } from '../../src/handlers/athena-health';

function envelope(name: string, data: unknown, _ms: number, extra: Record<string, unknown> = {}) {
  return { _meta: { name, ...extra }, data };
}

function emptyResult(): SparqlResult {
  return { results: { bindings: [] } };
}

function deps(overrides: Partial<DomainFacetDeps> = {}): DomainFacetDeps {
  return {
    sparql: async () => emptyResult(),
    envelope,
    now: () => 1000,
    ...overrides,
  };
}

// --- fetchDomainTests ---

describe('fetchDomainTests', () => {
  // #4093 — the registry (tests domain graph) is the source of truth; the scanner is
  // the fallback. Jeff, 2026-09-03: "the domains ultimately will show true state".
  test('#4093 registry rows win over the scanner: files listed once with their layer, total = registered cases', async () => {
    let scannerCalled = false;
    const d = deps({
      sparql: async (q: string) => q.includes('urn:chorus:domains:tests')
        ? { results: { bindings: [
            { f: { value: 'platform/tests/a.bats' }, l: { value: 'integration' }, n: { value: '3' } },
            { f: { value: 'platform/api/tests/b.test.ts' }, l: { value: 'unit' }, n: { value: '12' } },
          ] } }
        : emptyResult(),
      qualityByDomain: async () => { scannerCalled = true; return { layers: [{ key: 'unit', files: [{ name: 'stale.ts' }] }], total: 1 }; },
    });
    const r = await fetchDomainTests(d, 'tests');
    const body = r.body as { data: { tests: Array<{ path: string; type: string }>; byType: Record<string, number>; total: number } };
    expect(scannerCalled).toBe(false);
    expect(body.data.tests.map((x) => x.path)).toEqual(['platform/tests/a.bats', 'platform/api/tests/b.test.ts']);
    expect(body.data.byType).toEqual({ integration: 1, unit: 1 });
    expect(body.data.total).toBe(15);
  });

  test('#4093 NEGATIVE PROOF: a registry that answers nothing falls back to the scanner, never to an empty page', async () => {
    const d = deps({
      sparql: async () => emptyResult(),
      qualityByDomain: async () => ({ layers: [{ key: 'unit', files: [{ name: 'scanned.ts' }] }], total: 1 }),
    });
    const r = await fetchDomainTests(d, 'tests');
    const body = r.body as { data: { tests: Array<{ path: string }> } };
    expect(body.data.tests.map((x) => x.path)).toEqual(['scanned.ts']);
  });

  test('#4093 a registry that THROWS is not a 500: the scanner answers', async () => {
    const d = deps({
      sparql: async () => { throw new Error('fuseki down'); },
      qualityByDomain: async () => ({ layers: [{ key: 'bdd', files: [{ name: 'x.feature' }] }], total: 1 }),
    });
    const r = await fetchDomainTests(d, 'tests');
    expect(r.status).toBe(200);
    expect((r.body as { data: { tests: unknown[] } }).data.tests.length).toBe(1);
  });

  test('projection layers shape into tests array with layer as type (#3657)', async () => {
    const d = deps({
      qualityByDomain: async () => ({
        layers: [
          { key: 'unit', files: [{ name: 'a.test.ts' }] },
          { key: 'integration', files: [{ name: 'b.bats' }] },
        ],
        total: 42,
      }),
    });
    const r = await fetchDomainTests(d, 'awareness');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: Array<{ path: string; type: string }>; byType: Record<string, number>; total: number } };
    expect(body.data.tests.length).toBe(2);
    expect(body.data.byType.unit).toBe(1);
    expect(body.data.byType.integration).toBe(1);
    expect(body.data.total).toBe(42);
  });

  test('scanner throws returns empty envelope (200, no 500)', async () => {
    const d = deps({ qualityByDomain: async () => { throw new Error('scan failed'); } });
    const r = await fetchDomainTests(d, 'awareness');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: unknown[] } };
    expect(body.data.tests).toEqual([]);
  });

  test('strips -domain suffix before calling the scanner (#3656: local, no HTTP)', async () => {
    let seenDomain = '';
    const d = deps({
      qualityByDomain: async (domain: string) => { seenDomain = domain; return { layers: [] }; },
    });
    await fetchDomainTests(d, 'photos-domain');
    expect(seenDomain).toBe('photos');
  });

  test('#2485 — falls back to chorus:TestCoverage SPARQL when scanner has no data', async () => {
    const sparqlResult: SparqlResult = {
      results: {
        bindings: [
          { testFile: { value: 'chorus/platform/api/tests/seed-loom-decisions.test.ts' }, testType: { value: 'unit' } },
          { testFile: { value: 'chorus/platform/tests/decisions-graph.test.sh' }, testType: { value: 'integration' } },
        ],
      },
    };
    const d = deps({
      qualityByDomain: async () => ({ layers: [], total: 0 }),
      sparql: async () => sparqlResult,
    });
    const r = await fetchDomainTests(d, 'loom-decisions');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: Array<{ path: string; type: string }>; byType: Record<string, number> } };
    expect(body.data.tests.length).toBe(2);
    expect(body.data.tests[0].path).toContain('seed-loom-decisions');
    expect(body.data.byType.unit).toBe(1);
    expect(body.data.byType.integration).toBe(1);
  });

  test('#3442: fallback query reads testType via hasProperty→Property, NOT a bare literal', async () => {
    // Regression guard: the fallback above mocks the sparql RESPONSE, so a
    // revert to `chorus:testType ?testType` would still pass it and silently go
    // empty against the promoted graph. Assert the query SHAPE so revert → red.
    let q = '';
    const d = deps({
      qualityByDomain: async () => ({ layers: [], total: 0 }),
      sparql: async (qq: string) => { q = qq; return { results: { bindings: [] } } as SparqlResult; },
    });
    await fetchDomainTests(d, 'loom-decisions');
    expect(q).toContain('chorus:hasProperty');
    expect(q).toContain('chorus:propertyKey "testType"');
    expect(q).toContain('chorus:propertyValue ?testType');
    expect(q).not.toMatch(/chorus:testType\s+\?testType/);
  });

  test('#2485 — scanner data takes precedence; SPARQL fallback only fires on empty scan', async () => {
    const sparqlResult: SparqlResult = {
      results: { bindings: [{ testFile: { value: 'chorus/x.test.ts' }, testType: { value: 'unit' } }] },
    };
    const d = deps({
      qualityByDomain: async () => ({ layers: [{ key: 'unit', files: [{ name: 'projected/y.test.ts' }] }], total: 1 }),
      sparql: async () => sparqlResult,
    });
    const r = await fetchDomainTests(d, 'awareness');
    const body = r.body as { data: { tests: Array<{ path: string; type: string }> } };
    expect(body.data.tests.length).toBe(1);
    expect(body.data.tests[0].path).toBe('projected/y.test.ts');
  });
});

// --- fetchDomainLogs: RETIRED by #4084 (Logs fold reads chorus:LogSource from the graph; see platform/tests/4084-log-harvest.bats) ---

describe('fetchDomainServices', () => {
  test('shapes endpoints and aggregates byMethod', async () => {
    const result: SparqlResult = {
      results: {
        bindings: [
          { method: { value: 'GET' }, routePath: { value: '/api/a' }, filePath: { value: 'server.ts' } },
          { method: { value: 'GET' }, routePath: { value: '/api/b' }, filePath: { value: 'server.ts' } },
          { method: { value: 'POST' }, routePath: { value: '/api/c' }, filePath: { value: 'server.ts' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainServices(d, 'awareness');
    const body = r.body as { data: { endpoints: unknown[]; byMethod: Record<string, number> } };
    expect(body.data.endpoints.length).toBe(3);
    expect(body.data.byMethod.GET).toBe(2);
    expect(body.data.byMethod.POST).toBe(1);
  });

  test('empty result gives empty endpoints + byMethod', async () => {
    const d = deps();
    const r = await fetchDomainServices(d, 'awareness');
    const body = r.body as { data: { endpoints: unknown[]; byMethod: Record<string, number> } };
    expect(body.data.endpoints).toEqual([]);
    expect(body.data.byMethod).toEqual({});
  });

});

// --- fetchDomainDecisions ---

describe('fetchDomainDecisions', () => {
  // #5021 — the fold reads the decisions domain's ADR and Approval rows, joined by hasDomain
  test('shapes ADR and Approval rows from the decisions domain graph', async () => {
    const C = 'https://jeffbridwell.com/chorus#';
    const result: SparqlResult = {
      results: {
        bindings: [
          { s: { value: `${C}adr-051` }, t: { value: `${C}ADR` }, title: { value: 'ADR-051 domains graphs' }, status: { value: 'accepted' } },
          { s: { value: `${C}approval-17` }, t: { value: `${C}Approval` }, title: { value: 'a yes' }, date: { value: '2026-11-18' } },
        ],
      },
    };
    let seen = '';
    const d = deps({ sparql: async (q) => { seen = q; return result; } });
    const r = await fetchDomainDecisions(d, 'principles');
    expect(r.status).toBe(200);
    const body = r.body as { data: { decisions: Array<{ id: string; type: string; title: string }> } };
    expect(body.data.decisions.map((x) => [x.id, x.type])).toEqual([['adr-051', 'ADR'], ['approval-17', 'Approval']]);
    expect(seen).toContain('GRAPH <urn:chorus:domains:decisions>');
    expect(seen).toContain('<https://jeffbridwell.com/chorus#principles>');
  });

  test('NEGATIVE: it never reads the retired urn:chorus:decisions graph or chorus:Decision', async () => {
    let seen = '';
    const d = deps({ sparql: async (q) => { seen = q; return emptyResult(); } });
    await fetchDomainDecisions(d, 'principles');
    expect(seen).not.toContain('<urn:chorus:decisions>');
    expect(seen).not.toContain('chorus:Decision ');
  });

  test('alias mapping: tests → quality also queried', async () => {
    let seenQuery = '';
    const d = deps({ sparql: async (q) => { seenQuery = q; return emptyResult(); } });
    await fetchDomainDecisions(d, 'tests');
    expect(seenQuery).toContain('tests-domain');
    expect(seenQuery).toContain('quality-domain');
  });

  test('NEGATIVE: a failed query is a 502, never an empty fold', async () => {
    const d = deps({ sparql: async () => { throw new Error('oops'); } });
    const r = await fetchDomainDecisions(d, 'awareness');
    expect(r.status).toBe(502);
  });
});

// --- fetchDomainAlerts ---

// #4285 — the Alerts fold reads the graph now; its tests live in athena-facets-read-domain-graphs-4285.test.ts

describe('fetchDomainInfra', () => {
  test('groups rows by envName and collects dependsOn', async () => {
    const result = {
      results: {
        bindings: [
          { env: { value: 'e1' }, name: { value: 'fuseki' }, port: { value: '3030' }, host: { value: 'library' }, engine: { value: 'jena' }, dep: { value: 'sqlite' } },
          { env: { value: 'e1' }, name: { value: 'fuseki' }, port: { value: '3030' }, host: { value: 'library' }, engine: { value: 'jena' }, dep: { value: 'filesystem' } },
          { env: { value: 'e2' }, name: { value: 'chorus-api' }, port: { value: '3340' }, host: { value: 'library' }, engine: { value: 'node' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainInfra(d, 'awareness');
    const body = r.body as { data: { environments: Array<{ name: string; dependsOn: string[] }> } };
    expect(body.data.environments.length).toBe(2);
    const fuseki = body.data.environments.find((e) => e.name === 'fuseki')!;
    expect(fuseki.dependsOn.sort()).toEqual(['filesystem', 'sqlite']);
    const api = body.data.environments.find((e) => e.name === 'chorus-api')!;
    expect(api.dependsOn).toEqual([]);
  });

  test('missing optional fields default to null', async () => {
    const result = {
      results: {
        bindings: [
          { env: { value: 'e1' }, name: { value: 'minimal' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainInfra(d, 'x');
    const body = r.body as { data: { environments: Array<{ port: string | null; health: string | null }> } };
    expect(body.data.environments[0].port).toBeNull();
    expect(body.data.environments[0].health).toBeNull();
  });

  test('throw returns empty envelope', async () => {
    const d = deps({ sparql: async () => { throw new Error(''); } });
    const r = await fetchDomainInfra(d, 'x');
    expect(r.status).toBe(200);
    const body = r.body as { data: { environments: unknown[] } };
    expect(body.data.environments).toEqual([]);
  });
});

// --- fetchDomainRadius ---

describe('fetchDomainRadius', () => {
  test('shapes edges with label/uri/relationship/direction', async () => {
    const result = {
      results: {
        bindings: [
          { target: { value: 'https://jeffbridwell.com/chorus#api-domain' }, label: { value: 'API' }, relationship: { value: 'consumes' }, direction: { value: 'outbound' } },
          { target: { value: 'https://jeffbridwell.com/chorus#storage-domain' }, relationship: { value: 'hasDomain' }, direction: { value: 'parent' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainRadius(d, 'awareness');
    const body = r.body as { data: { edges: Array<{ target: string; relationship: string; direction: string }> } };
    expect(body.data.edges.length).toBe(2);
    expect(body.data.edges[0].target).toBe('API');
    expect(body.data.edges[0].direction).toBe('outbound');
    expect(body.data.edges[1].target).toBe('storage-domain'); // label missing → uri tail
  });

  test('throw returns empty envelope', async () => {
    const d = deps({ sparql: async () => { throw new Error(''); } });
    const r = await fetchDomainRadius(d, 'x');
    expect(r.status).toBe(200);
    const body = r.body as { data: { edges: unknown[] } };
    expect(body.data.edges).toEqual([]);
  });
});

// --- fetchDomainBlastRadius ---

describe('fetchDomainBlastRadius', () => {
  test('dedups edges by target+relationship key', async () => {
    const result = {
      results: {
        bindings: [
          { target: { value: 'https://jeffbridwell.com/chorus#a' }, label: { value: 'A' }, relationship: { value: 'consumes' }, direction: { value: 'dependent' } },
          { target: { value: 'https://jeffbridwell.com/chorus#a' }, label: { value: 'A' }, relationship: { value: 'consumes' }, direction: { value: 'dependent' } },
          { target: { value: 'https://jeffbridwell.com/chorus#a' }, label: { value: 'A' }, relationship: { value: 'ownerProduct' }, direction: { value: 'parent' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainBlastRadius(d, 'awareness');
    const body = r.body as { data: { edges: unknown[] } };
    // duplicate of (a|consumes) should collapse; (a|ownerProduct) is distinct
    expect(body.data.edges.length).toBe(2);
  });

  test('empty bindings → empty envelope', async () => {
    const d = deps();
    const r = await fetchDomainBlastRadius(d, 'awareness');
    const body = r.body as { data: { edges: unknown[] } };
    expect(body.data.edges).toEqual([]);
  });

});
