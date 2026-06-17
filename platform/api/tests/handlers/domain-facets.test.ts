// @test-type: unit — signal:integration is fixture-data (handler tests with mocked fetch/sparql).
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
  fetchDomainLogs,
  fetchDomainServices,
  fetchDomainDecisions,
  fetchDomainRadius,
  fetchDomainBlastRadius,
  fetchDomainAlerts,
  fetchDomainInfra,
  type DomainFacetDeps,
  type DomainAlertsDeps,
} from '../../src/handlers/domain-facets';
import type { SparqlResult } from '../../src/handlers/athena-health';

function envelope(name: string, data: unknown, _ms: number, extra: Record<string, unknown> = {}) {
  return { _meta: { name, ...extra }, data };
}

function mockFetch(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as unknown as Response;
}

function emptyResult(): SparqlResult {
  return { results: { bindings: [] } };
}

function deps(overrides: Partial<DomainFacetDeps> = {}): DomainFacetDeps {
  return {
    sparql: async () => emptyResult(),
    resolveSubdomainId: async (n) => n,
    envelope,
    now: () => 1000,
    ...overrides,
  };
}

// --- fetchDomainTests ---

describe('fetchDomainTests', () => {
  test('upstream 200 shapes files into tests array', async () => {
    const d = deps({
      fetcher: async () => mockFetch(200, { files: [{ name: 'a.test.ts', kind: 'ts' }, { name: 'b.bats', kind: 'bats' }], total: 42 }),
    });
    const r = await fetchDomainTests(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: Array<{ path: string; type: string }>; byType: Record<string, number>; total: number } };
    expect(body.data.tests.length).toBe(2);
    expect(body.data.byType.ts).toBe(1);
    expect(body.data.byType.bats).toBe(1);
    expect(body.data.total).toBe(42);
  });

  test('upstream 500 returns empty envelope (200, no 500)', async () => {
    const d = deps({ fetcher: async () => mockFetch(500, {}) });
    const r = await fetchDomainTests(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: unknown[] } };
    expect(body.data.tests).toEqual([]);
  });

  test('fetcher throws returns empty envelope (200, no 500)', async () => {
    const d = deps({ fetcher: async () => { throw new Error('network'); } });
    const r = await fetchDomainTests(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { tests: unknown[] } };
    expect(body.data.tests).toEqual([]);
  });

  test('strips -domain suffix before calling upstream', async () => {
    let seenUrl = '';
    const d = deps({
      fetcher: async (url: string) => { seenUrl = url; return mockFetch(200, { files: [] }); },
    });
    await fetchDomainTests(d, 'chorus-domain');
    expect(seenUrl).toBe('http://localhost:3000/api/quality/domain/chorus');
  });

  test('#2485 — falls back to chorus:TestCoverage SPARQL when upstream has no data', async () => {
    const sparqlResult: SparqlResult = {
      results: {
        bindings: [
          { testFile: { value: 'chorus/platform/api/tests/seed-loom-decisions.test.ts' }, testType: { value: 'unit' } },
          { testFile: { value: 'chorus/platform/tests/decisions-graph.test.sh' }, testType: { value: 'integration' } },
        ],
      },
    };
    const d = deps({
      fetcher: async () => mockFetch(200, { files: [], total: 0 }),
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
      fetcher: async () => mockFetch(200, { files: [], total: 0 }),
      sparql: async (qq: string) => { q = qq; return { results: { bindings: [] } } as SparqlResult; },
    });
    await fetchDomainTests(d, 'loom-decisions');
    expect(q).toContain('chorus:hasProperty');
    expect(q).toContain('chorus:propertyKey "testType"');
    expect(q).toContain('chorus:propertyValue ?testType');
    expect(q).not.toMatch(/chorus:testType\s+\?testType/);
  });

  test('#2485 — upstream data takes precedence; SPARQL fallback only fires on empty upstream', async () => {
    const sparqlResult: SparqlResult = {
      results: { bindings: [{ testFile: { value: 'chorus/x.test.ts' }, testType: { value: 'unit' } }] },
    };
    const d = deps({
      fetcher: async () => mockFetch(200, { files: [{ name: 'gathering/y.test.ts', kind: 'ts' }], total: 1 }),
      sparql: async () => sparqlResult,
    });
    const r = await fetchDomainTests(d, 'chorus-domain');
    const body = r.body as { data: { tests: Array<{ path: string; type: string }> } };
    expect(body.data.tests.length).toBe(1);
    expect(body.data.tests[0].path).toBe('gathering/y.test.ts');
  });
});

// --- fetchDomainLogs ---

describe('fetchDomainLogs', () => {
  test('shapes SPARQL bindings into logs array', async () => {
    const result: SparqlResult = {
      results: {
        bindings: [
          { log: { value: 'https://jeffbridwell.com/chorus#api-log' }, label: { value: 'chorus-api' }, location: { value: '/var/log/chorus-api' }, status: { value: 'healthy' } },
          { log: { value: 'https://jeffbridwell.com/chorus#fuseki-log' }, label: { value: 'fuseki' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainLogs(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { logs: Array<{ label: string; location: string | null; status: string | null }> } };
    expect(body.data.logs.length).toBe(2);
    expect(body.data.logs[0].label).toBe('chorus-api');
    expect(body.data.logs[0].location).toBe('/var/log/chorus-api');
    expect(body.data.logs[1].location).toBeNull();
    expect(body.data.logs[1].status).toBeNull();
  });

  test('derives label from uri when label missing', async () => {
    const result: SparqlResult = {
      results: { bindings: [{ log: { value: 'https://jeffbridwell.com/chorus#nameless-log' } }] },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainLogs(d, 'x');
    const body = r.body as { data: { logs: Array<{ label: string }> } };
    expect(body.data.logs[0].label).toBe('nameless-log');
  });

  test('SPARQL throws returns empty envelope', async () => {
    const d = deps({ sparql: async () => { throw new Error('timeout'); } });
    const r = await fetchDomainLogs(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { logs: unknown[] } };
    expect(body.data.logs).toEqual([]);
  });
});

// --- fetchDomainServices ---

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
    const r = await fetchDomainServices(d, 'chorus-domain');
    const body = r.body as { data: { endpoints: unknown[]; byMethod: Record<string, number> } };
    expect(body.data.endpoints.length).toBe(3);
    expect(body.data.byMethod.GET).toBe(2);
    expect(body.data.byMethod.POST).toBe(1);
  });

  test('empty result gives empty endpoints + byMethod', async () => {
    const d = deps();
    const r = await fetchDomainServices(d, 'chorus-domain');
    const body = r.body as { data: { endpoints: unknown[]; byMethod: Record<string, number> } };
    expect(body.data.endpoints).toEqual([]);
    expect(body.data.byMethod).toEqual({});
  });

  test('resolveSubdomainId throw returns empty envelope', async () => {
    const d = deps({ resolveSubdomainId: async () => { throw new Error('no such subdomain'); } });
    const r = await fetchDomainServices(d, 'nonexistent');
    expect(r.status).toBe(200);
    const body = r.body as { data: { endpoints: unknown[] } };
    expect(body.data.endpoints).toEqual([]);
  });
});

// --- fetchDomainDecisions ---

describe('fetchDomainDecisions', () => {
  test('shapes decisions and aggregates byEnforcement', async () => {
    const result: SparqlResult = {
      results: {
        bindings: [
          { id: { value: 'DEC-001' }, title: { value: 'A' }, date: { value: '2026-01-01' }, status: { value: 'active' }, level: { value: 'binding' }, type: { value: 'architecture' } },
          { id: { value: 'DEC-002' }, title: { value: 'B' }, date: { value: '2026-02-01' }, status: { value: 'active' }, level: { value: 'binding' }, type: { value: 'process' } },
          { id: { value: 'DEC-003' }, title: { value: 'C' }, date: { value: '2026-03-01' }, status: { value: 'active' }, level: { value: 'advisory' }, type: { value: 'architecture' } },
        ],
      },
    };
    const d = deps({ sparql: async () => result });
    const r = await fetchDomainDecisions(d, 'chorus-domain');
    const body = r.body as { data: { decisions: unknown[]; byEnforcement: Record<string, number> } };
    expect(body.data.decisions.length).toBe(3);
    expect(body.data.byEnforcement.binding).toBe(2);
    expect(body.data.byEnforcement.advisory).toBe(1);
  });

  test('alias mapping: tests → quality also queried', async () => {
    let seenQuery = '';
    const d = deps({ sparql: async (q) => { seenQuery = q; return emptyResult(); } });
    await fetchDomainDecisions(d, 'tests');
    expect(seenQuery).toContain('tests-domain');
    expect(seenQuery).toContain('quality-domain');
  });

  test('SPARQL throws returns empty envelope', async () => {
    const d = deps({ sparql: async () => { throw new Error('oops'); } });
    const r = await fetchDomainDecisions(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { decisions: unknown[] } };
    expect(body.data.decisions).toEqual([]);
  });
});

// --- fetchDomainAlerts ---

function alertsDeps(overrides: Partial<DomainAlertsDeps> = {}): DomainAlertsDeps {
  return {
    ...deps(),
    readAlertFiles: () => [],
    ...overrides,
  };
}

describe('fetchDomainAlerts', () => {
  test('parses name/description/severity/schedule from matching yml', async () => {
    const d = alertsDeps({
      readAlertFiles: () => [
        {
          file: 'chorus-hooks-down.yml',
          content: 'name: chorus-hooks process died\ndescription: hook daemon stopped\nseverity: critical\nschedule: "@every 60s"\n',
        },
      ],
    });
    const r = await fetchDomainAlerts(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { alerts: Array<{ name: string; severity: string; schedule: string }> } };
    expect(body.data.alerts.length).toBe(1);
    expect(body.data.alerts[0].name).toBe('chorus-hooks process died');
    expect(body.data.alerts[0].severity).toBe('critical');
    expect(body.data.alerts[0].schedule).toBe('@every 60s');
  });

  test('matches on domain label in filename when content does not mention it', async () => {
    const d = alertsDeps({
      readAlertFiles: () => [
        { file: 'chorus-alert.yml', content: 'name: unrelated\ndescription: x\nseverity: warn\nschedule: "*/5 * * * *"' },
      ],
    });
    const r = await fetchDomainAlerts(d, 'chorus-domain');
    const body = r.body as { data: { alerts: unknown[] } };
    expect(body.data.alerts.length).toBe(1);
  });

  test('skips files that do not mention the domain', async () => {
    const d = alertsDeps({
      readAlertFiles: () => [
        { file: 'unrelated.yml', content: 'name: something\ndescription: x\nseverity: warn' },
      ],
    });
    const r = await fetchDomainAlerts(d, 'chorus-domain');
    const body = r.body as { data: { alerts: unknown[] } };
    expect(body.data.alerts).toEqual([]);
  });

  test('missing fields default sensibly', async () => {
    const d = alertsDeps({
      readAlertFiles: () => [
        { file: 'chorus-bare.yml', content: '# chorus alert with no structured fields\n' },
      ],
    });
    const r = await fetchDomainAlerts(d, 'chorus-domain');
    const body = r.body as { data: { alerts: Array<{ name: string; severity: string }> } };
    expect(body.data.alerts[0].name).toBe('chorus-bare'); // file minus .yml
    expect(body.data.alerts[0].severity).toBe('unknown');
  });

  test('readAlertFiles throw returns empty envelope', async () => {
    const d = alertsDeps({
      readAlertFiles: () => { throw new Error('EACCES'); },
    });
    const r = await fetchDomainAlerts(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { alerts: unknown[] } };
    expect(body.data.alerts).toEqual([]);
  });
});

// --- fetchDomainInfra ---

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
    const r = await fetchDomainInfra(d, 'chorus-domain');
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
    const r = await fetchDomainRadius(d, 'chorus-domain');
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
    const r = await fetchDomainBlastRadius(d, 'chorus-domain');
    const body = r.body as { data: { edges: unknown[] } };
    // duplicate of (a|consumes) should collapse; (a|ownerProduct) is distinct
    expect(body.data.edges.length).toBe(2);
  });

  test('empty bindings → empty envelope', async () => {
    const d = deps();
    const r = await fetchDomainBlastRadius(d, 'chorus-domain');
    const body = r.body as { data: { edges: unknown[] } };
    expect(body.data.edges).toEqual([]);
  });

  test('resolveSubdomainId throw returns empty envelope', async () => {
    const d = deps({ resolveSubdomainId: async () => { throw new Error(''); } });
    const r = await fetchDomainBlastRadius(d, 'nonexistent');
    expect(r.status).toBe(200);
    const body = r.body as { data: { edges: unknown[] } };
    expect(body.data.edges).toEqual([]);
  });
});
