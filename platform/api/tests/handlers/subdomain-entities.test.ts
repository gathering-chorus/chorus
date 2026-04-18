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
  createSubdomainService,
  createSubdomainPipeline,
  createSubdomainLog,
  createSubdomainGap,
  createSubdomainPage,
  createSubdomainIntegration,
  createSubdomainPersistence,
  createSubdomainScenario,
  deleteSubdomainEntity,
  updateSubdomainService,
  updateSubdomainPipeline,
  updateSubdomainPage,
  updateSubdomainIntegration,
  updateSubdomainPersistence,
  type WriteDeps,
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

// --- POST create handlers ---

function writeDeps(overrides: Partial<WriteDeps> = {}): WriteDeps & { lastUpdate: { value: string } } {
  const lastUpdate = { value: '' };
  const base: WriteDeps = {
    ...deps(),
    sparqlUpdate: async (u: string) => { lastUpdate.value = u; },
    ...overrides,
  };
  return { ...base, lastUpdate };
}

describe('createSubdomainService', () => {
  test('missing label returns 400', async () => {
    const d = writeDeps();
    const r = await createSubdomainService(d, 'chorus-domain', {});
    expect(r.status).toBe(400);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toContain('Missing required field: label');
  });

  test('null body treated as missing label', async () => {
    const d = writeDeps();
    const r = await createSubdomainService(d, 'chorus-domain', null);
    expect(r.status).toBe(400);
  });

  test('happy path returns 200 with uri + echo of props', async () => {
    const d = writeDeps();
    const r = await createSubdomainService(d, 'chorus-domain', {
      label: 'chorus-api',
      type: 'http',
      host: 'localhost',
      status: 'healthy',
      health_endpoint: '/health',
    });
    expect(r.status).toBe(200);
    const body = r.body as { data: { uri: string; label: string; type: string; host: string } };
    expect(body.data.uri).toBe('https://jeffbridwell.com/chorus#chorus-domain-service-chorus-api');
    expect(body.data.label).toBe('chorus-api');
    expect(body.data.type).toBe('http');
    expect(body.data.host).toBe('localhost');
  });

  test('slugifies label into URI (spaces → hyphens, lowercased)', async () => {
    const d = writeDeps();
    const r = await createSubdomainService(d, 'chorus-domain', { label: 'My Fancy Service' });
    const body = r.body as { data: { uri: string } };
    expect(body.data.uri).toBe('https://jeffbridwell.com/chorus#chorus-domain-service-my-fancy-service');
  });

  test('escapes double quotes in label to avoid SPARQL injection', async () => {
    const d = writeDeps();
    await createSubdomainService(d, 'chorus-domain', { label: 'evil "quoted" name' });
    expect(d.lastUpdate.value).toContain('rdfs:label "evil \\"quoted\\" name"');
  });

  test('omits property triples for undefined fields', async () => {
    const d = writeDeps();
    await createSubdomainService(d, 'chorus-domain', { label: 'bare' });
    expect(d.lastUpdate.value).not.toContain('chorus:serviceType');
    expect(d.lastUpdate.value).not.toContain('chorus:serviceHost');
    expect(d.lastUpdate.value).toContain('chorus:hasService');
  });

  test('update throw maps to 500 with error body', async () => {
    const d = writeDeps({
      sparqlUpdate: async () => { throw new Error('fuseki write failed'); },
    });
    const r = await createSubdomainService(d, 'chorus-domain', { label: 'x' });
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('fuseki write failed');
  });

  test('update contains chorus:Service type and hasService edge', async () => {
    const d = writeDeps();
    await createSubdomainService(d, 'chorus-domain', { label: 'svc' });
    expect(d.lastUpdate.value).toContain('a chorus:Service');
    expect(d.lastUpdate.value).toContain('chorus:hasService');
  });
});

describe('createSubdomainPipeline', () => {
  test('includes chorus:Pipeline type + pipeline-specific predicates', async () => {
    const d = writeDeps();
    await createSubdomainPipeline(d, 'chorus-domain', {
      label: 'kg sync',
      source: 'fuseki',
      harvester: 'nifi',
    });
    expect(d.lastUpdate.value).toContain('a chorus:Pipeline');
    expect(d.lastUpdate.value).toContain('chorus:hasPipeline');
    expect(d.lastUpdate.value).toContain('chorus:pipelineSource "fuseki"');
    expect(d.lastUpdate.value).toContain('chorus:pipelineHarvester "nifi"');
  });

  test('missing label returns 400', async () => {
    const d = writeDeps();
    const r = await createSubdomainPipeline(d, 'chorus-domain', {});
    expect(r.status).toBe(400);
  });
});

describe('createSubdomainLog', () => {
  test('includes chorus:LogSource type + log-specific predicates', async () => {
    const d = writeDeps();
    await createSubdomainLog(d, 'chorus-domain', {
      label: 'api-log',
      location: '/var/log/chorus-api',
      retention: '7d',
    });
    expect(d.lastUpdate.value).toContain('a chorus:LogSource');
    expect(d.lastUpdate.value).toContain('chorus:hasLogSource');
    expect(d.lastUpdate.value).toContain('chorus:logSourceLocation "/var/log/chorus-api"');
    expect(d.lastUpdate.value).toContain('chorus:logSourceRetention "7d"');
  });
});

describe('createSubdomainGap', () => {
  test('includes chorus:Gap type + gap-specific predicates', async () => {
    const d = writeDeps();
    await createSubdomainGap(d, 'chorus-domain', {
      label: 'no e2e tests',
      severity: 'high',
      description: 'Missing end-to-end test coverage',
    });
    expect(d.lastUpdate.value).toContain('a chorus:Gap');
    expect(d.lastUpdate.value).toContain('chorus:hasGap');
    expect(d.lastUpdate.value).toContain('chorus:gapSeverity "high"');
    expect(d.lastUpdate.value).toContain('chorus:gapDescription "Missing end-to-end test coverage"');
  });

  test('URI segment is "gap" (not "gaps")', async () => {
    const d = writeDeps();
    const r = await createSubdomainGap(d, 'chorus-domain', { label: 'x' });
    const body = r.body as { data: { uri: string } };
    expect(body.data.uri).toContain('-gap-x');
  });
});

// --- More POST create kinds ---

describe('createSubdomainPage', () => {
  test('uses chorus:Page + hasPage + pageRoute', async () => {
    const d = writeDeps();
    await createSubdomainPage(d, 'chorus-domain', { label: 'home', route: '/' });
    expect(d.lastUpdate.value).toContain('a chorus:Page');
    expect(d.lastUpdate.value).toContain('chorus:hasPage');
    expect(d.lastUpdate.value).toContain('chorus:pageRoute "/"');
  });
});

describe('createSubdomainIntegration', () => {
  test('uses chorus:Integration + body.path → integrationPath', async () => {
    const d = writeDeps();
    await createSubdomainIntegration(d, 'chorus-domain', {
      label: 'vikunja',
      source: 'api',
      path: '/api/v1/projects',
    });
    expect(d.lastUpdate.value).toContain('a chorus:Integration');
    expect(d.lastUpdate.value).toContain('chorus:integrationPath "/api/v1/projects"');
  });
});

describe('createSubdomainPersistence', () => {
  test('serializes numeric records as string literal', async () => {
    const d = writeDeps();
    await createSubdomainPersistence(d, 'chorus-domain', {
      label: 'sqlite',
      type: 'sqlite',
      records: 4200,
    });
    expect(d.lastUpdate.value).toContain('chorus:storeRecordCount "4200"');
  });

  test('preserves records as-is in echo body', async () => {
    const d = writeDeps();
    const r = await createSubdomainPersistence(d, 'chorus-domain', {
      label: 'sqlite',
      records: 4200,
    });
    const body = r.body as { data: { records: number } };
    expect(body.data.records).toBe(4200);
  });
});

describe('createSubdomainScenario', () => {
  test('uses chorus:Scenario + Given/When/Then predicates', async () => {
    const d = writeDeps();
    await createSubdomainScenario(d, 'chorus-domain', {
      label: 'login',
      given: 'user on homepage',
      when: 'clicks sign in',
      then: 'lands on dashboard',
    });
    expect(d.lastUpdate.value).toContain('a chorus:Scenario');
    expect(d.lastUpdate.value).toContain('chorus:scenarioGiven "user on homepage"');
    expect(d.lastUpdate.value).toContain('chorus:scenarioWhen "clicks sign in"');
    expect(d.lastUpdate.value).toContain('chorus:scenarioThen "lands on dashboard"');
  });

  test('missing label returns 400', async () => {
    const d = writeDeps();
    const r = await createSubdomainScenario(d, 'chorus-domain', {});
    expect(r.status).toBe(400);
  });
});

// --- DELETE handler (generic section dispatch) ---

describe('deleteSubdomainEntity', () => {
  test('known section → 204 no-content + DELETE fires', async () => {
    const d = writeDeps();
    const r = await deleteSubdomainEntity(d, 'chorus-domain', 'services', 'some-svc');
    expect(r.status).toBe(204);
    expect(r.body).toBeNull();
    expect(d.lastUpdate.value).toContain('DELETE');
    expect(d.lastUpdate.value).toContain('<https://jeffbridwell.com/chorus#some-svc>');
    expect(d.lastUpdate.value).toContain('chorus:hasService');
  });

  test('unknown section → 400 with helpful error', async () => {
    const d = writeDeps();
    const r = await deleteSubdomainEntity(d, 'chorus-domain', 'widgets', 'some-widget');
    expect(r.status).toBe(400);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toContain('Unknown section: widgets');
    // And the runner should NOT be called.
    expect(d.lastUpdate.value).toBe('');
  });

  test('each known section maps to the right hasPredicate', async () => {
    const sections: Array<[string, string]> = [
      ['actors', 'chorus:hasActor'],
      ['scenarios', 'chorus:hasScenario'],
      ['pages', 'chorus:hasPage'],
      ['integrations', 'chorus:hasIntegration'],
      ['persistence', 'chorus:hasPersistence'],
      ['pipeline', 'chorus:hasPipeline'],
      ['logs', 'chorus:hasLogSource'],
      ['gaps', 'chorus:hasGap'],
    ];
    for (const [section, predicate] of sections) {
      const d = writeDeps();
      await deleteSubdomainEntity(d, 'chorus-domain', section, 'some-entity');
      expect(d.lastUpdate.value).toContain(predicate);
    }
  });

  test('sparqlUpdate throw → 500 with error message', async () => {
    const d: WriteDeps = {
      ...deps(),
      sparqlUpdate: async () => { throw new Error('delete race'); },
    };
    const r = await deleteSubdomainEntity(d, 'chorus-domain', 'services', 'some-svc');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('delete race');
  });
});

// --- PUT update handlers ---

function writeDepsMulti(): WriteDeps & { updates: string[] } {
  const updates: string[] = [];
  const base: WriteDeps = {
    ...deps(),
    sparqlUpdate: async (u: string) => { updates.push(u); },
  };
  return { ...base, updates };
}

describe('updateSubdomainService (PUT)', () => {
  test('missing label returns 400', async () => {
    const d = writeDepsMulti();
    const r = await updateSubdomainService(d, 'chorus-domain', 'my-svc', {});
    expect(r.status).toBe(400);
  });

  test('happy path fires DELETE then INSERT', async () => {
    const d = writeDepsMulti();
    const r = await updateSubdomainService(d, 'chorus-domain', 'my-svc', {
      label: 'renamed',
      type: 'grpc',
    });
    expect(r.status).toBe(200);
    expect(d.updates.length).toBe(2);
    expect(d.updates[0]).toContain('DELETE');
    expect(d.updates[0]).toContain('<https://jeffbridwell.com/chorus#my-svc>');
    expect(d.updates[1]).toContain('INSERT DATA');
    expect(d.updates[1]).toContain('a chorus:Service');
    expect(d.updates[1]).toContain('rdfs:label "renamed"');
    expect(d.updates[1]).toContain('chorus:serviceType "grpc"');
  });

  test('uses entityId as-is, not slugified label', async () => {
    const d = writeDepsMulti();
    // entityId already set (e.g. 'chorus-domain-service-old-name'); label can
    // change without renaming the URI.
    const r = await updateSubdomainService(d, 'chorus-domain', 'chorus-domain-service-old-name', {
      label: 'Completely New Label',
    });
    const body = r.body as { data: { uri: string } };
    expect(body.data.uri).toBe('https://jeffbridwell.com/chorus#chorus-domain-service-old-name');
  });

  test('DELETE throw propagates to 500', async () => {
    let call = 0;
    const d: WriteDeps = {
      ...deps(),
      sparqlUpdate: async () => {
        call++;
        if (call === 1) throw new Error('delete failed');
      },
    };
    const r = await updateSubdomainService(d, 'chorus-domain', 'x', { label: 'y' });
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('delete failed');
  });

  test('INSERT throw (after DELETE succeeds) propagates to 500', async () => {
    let call = 0;
    const d: WriteDeps = {
      ...deps(),
      sparqlUpdate: async () => {
        call++;
        if (call === 2) throw new Error('insert failed');
      },
    };
    const r = await updateSubdomainService(d, 'chorus-domain', 'x', { label: 'y' });
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('insert failed');
  });
});

describe('updateSubdomainPage', () => {
  test('uses chorus:Page type and chorus:hasPage edge', async () => {
    const d = writeDepsMulti();
    await updateSubdomainPage(d, 'chorus-domain', 'some-page', {
      label: 'home',
      route: '/',
    });
    expect(d.updates[1]).toContain('a chorus:Page');
    expect(d.updates[1]).toContain('chorus:hasPage');
    expect(d.updates[1]).toContain('chorus:pageRoute "/"');
  });
});

describe('updateSubdomainIntegration', () => {
  test('maps body.path → chorus:integrationPath', async () => {
    const d = writeDepsMulti();
    await updateSubdomainIntegration(d, 'chorus-domain', 'some-int', {
      label: 'vikunja',
      source: 'api',
      path: '/api/v1/projects',
    });
    expect(d.updates[1]).toContain('chorus:integrationSource "api"');
    expect(d.updates[1]).toContain('chorus:integrationPath "/api/v1/projects"');
  });
});

describe('updateSubdomainPersistence', () => {
  test('serializes numeric records as string literal', async () => {
    const d = writeDepsMulti();
    await updateSubdomainPersistence(d, 'chorus-domain', 'some-store', {
      label: 'sqlite',
      type: 'sqlite',
      records: 4200,
    });
    expect(d.updates[1]).toContain('chorus:storeRecordCount "4200"');
  });

  test('preserves records as-is in echo body', async () => {
    const d = writeDepsMulti();
    const r = await updateSubdomainPersistence(d, 'chorus-domain', 'some-store', {
      label: 'sqlite',
      records: 4200,
    });
    const body = r.body as { data: { records: number } };
    expect(body.data.records).toBe(4200);
  });
});

describe('updateSubdomainPipeline', () => {
  test('pipeline-specific predicates land in INSERT', async () => {
    const d = writeDepsMulti();
    await updateSubdomainPipeline(d, 'chorus-domain', 'some-pipe', {
      label: 'sync',
      source: 'fuseki',
      icd: 'ingest-v1',
    });
    expect(d.updates[1]).toContain('chorus:pipelineSource "fuseki"');
    expect(d.updates[1]).toContain('chorus:pipelineICD "ingest-v1"');
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
