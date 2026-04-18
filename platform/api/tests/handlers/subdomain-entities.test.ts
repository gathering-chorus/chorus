/**
 * subdomain-entities handlers — unit tests (#2180).
 *
 * Four list-GET handlers sharing one implementation. Tests prove
 * Jeff-visible behavior:
 *   - 404 when the subdomain doesn't exist (not 500, not empty-array)
 *   - 200 with shaped entities when it does
 *   - missing labels fall back to URI tail
 *   - 500 on SPARQL throw
 *   - spec-level wiring: each entity kind queries the right predicate
 */

import {
  fetchSubdomainServicesList,
  fetchSubdomainPipelineList,
  fetchSubdomainLogsList,
  fetchSubdomainGapsList,
  fetchSubdomainEntities,
  servicesSpec,
  pipelineSpec,
} from '../../src/handlers/subdomain-entities';
import type { DomainFacetDeps } from '../../src/handlers/domain-facets';
import type { SparqlResult } from '../../src/handlers/athena-health';

function envelope(name: string, data: unknown, _ms: number, extra: Record<string, unknown> = {}) {
  return { _meta: { name, ...extra }, data };
}

/**
 * Sparql stub that returns different results based on whether the query is
 * the existence check or the list fetch. The existence check has the
 * literal 'SELECT ?s' and the list has the entity variable.
 */
function sparqlStub(opts: {
  exists: boolean;
  listResult?: SparqlResult;
  throwOn?: 'exists' | 'list';
}): (q: string) => Promise<SparqlResult> {
  return async (q: string) => {
    const isCheck = q.includes('SELECT ?s WHERE') && q.includes('chorus:SubDomain');
    if (isCheck) {
      if (opts.throwOn === 'exists') throw new Error('fuseki down');
      return {
        results: { bindings: opts.exists ? [{ s: { value: 'stub' } }] : [] },
      };
    }
    if (opts.throwOn === 'list') throw new Error('list query failed');
    return opts.listResult || { results: { bindings: [] } };
  };
}

function deps(overrides: Partial<DomainFacetDeps> = {}): DomainFacetDeps {
  return {
    sparql: sparqlStub({ exists: true }),
    resolveSubdomainId: async (n) => n,
    envelope,
    now: () => 1000,
    ...overrides,
  };
}

// --- fetchSubdomainEntities (core) ---

describe('fetchSubdomainEntities', () => {
  test('returns 404 when subdomain does not exist', async () => {
    const d = deps({ sparql: sparqlStub({ exists: false }) });
    const r = await fetchSubdomainEntities(d, 'no-such-domain', servicesSpec);
    expect(r.status).toBe(404);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toContain("'no-such-domain' not found");
  });

  test('returns 200 with shaped entities when subdomain exists', async () => {
    const d = deps({
      sparql: sparqlStub({
        exists: true,
        listResult: {
          results: {
            bindings: [
              { svc: { value: 'https://jeffbridwell.com/chorus#api-svc' }, label: { value: 'chorus-api' }, type: { value: 'http' }, host: { value: 'localhost' } },
              { svc: { value: 'https://jeffbridwell.com/chorus#fuseki-svc' }, label: { value: 'fuseki' } },
            ],
          },
        },
      }),
    });
    const r = await fetchSubdomainServicesList(d, 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { services: Array<{ label: string; type: string | null; host: string | null }> } };
    expect(body.data.services.length).toBe(2);
    expect(body.data.services[0].label).toBe('chorus-api');
    expect(body.data.services[0].type).toBe('http');
    expect(body.data.services[1].type).toBeNull();
  });

  test('falls back to uri tail when label missing', async () => {
    const d = deps({
      sparql: sparqlStub({
        exists: true,
        listResult: {
          results: { bindings: [{ svc: { value: 'https://jeffbridwell.com/chorus#nameless-svc' } }] },
        },
      }),
    });
    const r = await fetchSubdomainServicesList(d, 'chorus-domain');
    const body = r.body as { data: { services: Array<{ label: string }> } };
    expect(body.data.services[0].label).toBe('nameless-svc');
  });

  test('returns 500 with error message when list SPARQL throws', async () => {
    const d = deps({ sparql: sparqlStub({ exists: true, throwOn: 'list' }) });
    const r = await fetchSubdomainServicesList(d, 'chorus-domain');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('list query failed');
  });

  test('returns 500 when existence check itself throws', async () => {
    const d = deps({ sparql: sparqlStub({ exists: true, throwOn: 'exists' }) });
    const r = await fetchSubdomainServicesList(d, 'chorus-domain');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('fuseki down');
  });
});

// --- Per-kind wiring tests: each spec queries the right predicate ---

describe('per-kind predicate wiring', () => {
  test('services uses chorus:hasService', async () => {
    let lastQuery = '';
    const d = deps({
      sparql: async (q) => {
        lastQuery = q;
        return q.includes('SELECT ?s WHERE')
          ? { results: { bindings: [{ s: { value: 'exists' } }] } }
          : { results: { bindings: [] } };
      },
    });
    await fetchSubdomainServicesList(d, 'x');
    expect(lastQuery).toContain('chorus:hasService');
    expect(lastQuery).toContain('?svc');
  });

  test('pipeline uses chorus:hasPipeline', async () => {
    let lastQuery = '';
    const d = deps({
      sparql: async (q) => {
        lastQuery = q;
        return q.includes('SELECT ?s WHERE')
          ? { results: { bindings: [{ s: { value: 'exists' } }] } }
          : { results: { bindings: [] } };
      },
    });
    await fetchSubdomainPipelineList(d, 'x');
    expect(lastQuery).toContain('chorus:hasPipeline');
    expect(lastQuery).toContain('?pipe');
  });

  test('logs uses chorus:hasLogSource', async () => {
    let lastQuery = '';
    const d = deps({
      sparql: async (q) => {
        lastQuery = q;
        return q.includes('SELECT ?s WHERE')
          ? { results: { bindings: [{ s: { value: 'exists' } }] } }
          : { results: { bindings: [] } };
      },
    });
    await fetchSubdomainLogsList(d, 'x');
    expect(lastQuery).toContain('chorus:hasLogSource');
    expect(lastQuery).toContain('?log');
  });

  test('gaps uses chorus:hasGap', async () => {
    let lastQuery = '';
    const d = deps({
      sparql: async (q) => {
        lastQuery = q;
        return q.includes('SELECT ?s WHERE')
          ? { results: { bindings: [{ s: { value: 'exists' } }] } }
          : { results: { bindings: [] } };
      },
    });
    await fetchSubdomainGapsList(d, 'x');
    expect(lastQuery).toContain('chorus:hasGap');
    expect(lastQuery).toContain('?gap');
  });
});

// --- Per-kind shape tests: each envelope has the right results key ---

describe('per-kind envelope wiring', () => {
  test('services envelope has services[] key', async () => {
    const d = deps({
      sparql: async (q) => (q.includes('chorus:SubDomain')
        ? { results: { bindings: [{ s: { value: 'exists' } }] } }
        : { results: { bindings: [{ svc: { value: 'https://j.c/c#s1' } }] } }),
    });
    const r = await fetchSubdomainServicesList(d, 'x');
    const body = r.body as { data: { services: unknown[] } };
    expect(Array.isArray(body.data.services)).toBe(true);
  });

  test('pipeline envelope has pipelines[] key', async () => {
    const d = deps({
      sparql: async (q) => (q.includes('chorus:SubDomain')
        ? { results: { bindings: [{ s: { value: 'exists' } }] } }
        : { results: { bindings: [{ pipe: { value: 'https://j.c/c#p1' } }] } }),
    });
    const r = await fetchSubdomainPipelineList(d, 'x');
    const body = r.body as { data: { pipelines: unknown[] } };
    expect(Array.isArray(body.data.pipelines)).toBe(true);
  });

  test('logs envelope has logs[] key', async () => {
    const d = deps({
      sparql: async (q) => (q.includes('chorus:SubDomain')
        ? { results: { bindings: [{ s: { value: 'exists' } }] } }
        : { results: { bindings: [{ log: { value: 'https://j.c/c#l1' } }] } }),
    });
    const r = await fetchSubdomainLogsList(d, 'x');
    const body = r.body as { data: { logs: unknown[] } };
    expect(Array.isArray(body.data.logs)).toBe(true);
  });

  test('gaps envelope has gaps[] key', async () => {
    const d = deps({
      sparql: async (q) => (q.includes('chorus:SubDomain')
        ? { results: { bindings: [{ s: { value: 'exists' } }] } }
        : { results: { bindings: [{ gap: { value: 'https://j.c/c#g1' } }] } }),
    });
    const r = await fetchSubdomainGapsList(d, 'x');
    const body = r.body as { data: { gaps: unknown[] } };
    expect(Array.isArray(body.data.gaps)).toBe(true);
  });
});
