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
  type DomainFacetDeps,
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
