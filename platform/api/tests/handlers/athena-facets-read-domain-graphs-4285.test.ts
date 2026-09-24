// @test-type: unit
// #4285 — the Athena domain folds read the graph the rows live in. On
// 2026-09-24 the tests domain showed Code 0 / API Contract 0 / Alerts 0 while
// the store held 626 CodeFile, 10 Endpoint and 6 Alert rows with hasDomain =
// tests: the facet queries still asked the retired urn:chorus:instances graph
// through inverse edges (hasCodeFile / hasEndpoint / hasPage), and the Alerts
// fold scanned YAML files by keyword. Hermetic: the sparql dep records the
// query it was asked and answers a fixture.
import { fetchAthenaSubdomainCode } from '../../src/handlers/athena-subdomain-code';
import { fetchAthenaSubdomainEndpoints } from '../../src/handlers/athena-subdomain-endpoints';
import { fetchAthenaSubdomainPages } from '../../src/handlers/athena-subdomain-pages';
import { fetchDomainAlerts } from '../../src/handlers/domain-facets';

const TESTS = 'https://jeffbridwell.com/chorus#tests';
const v = (value: string) => ({ value });

function recorder<B>(bindings: B[]) {
  const asked: string[] = [];
  return { asked, sparql: async (q: string) => { asked.push(q); return { results: { bindings } }; } };
}

describe('#4285 Code fold', () => {
  it('asks the code graph for CodeFile rows by hasDomain, never the retired instances graph', async () => {
    const r = recorder([
      { file: v('https://jeffbridwell.com/chorus#codefile-a'), filePath: v('platform/services/werk-test/src/lib.rs'), fileType: v('rs') },
      { file: v('https://jeffbridwell.com/chorus#codefile-b'), filePath: v('platform/api/src/handlers/nightly-report.ts'), fileType: v('ts') },
      { file: v('https://jeffbridwell.com/chorus#codefile-c'), filePath: v('platform/tests/4283-quartet-records-each-api.bats'), fileType: v('bats') },
    ]);
    const res = await fetchAthenaSubdomainCode({ sparql: r.sparql as never, extname: (p) => p.slice(p.lastIndexOf('.')), now: () => 0 }, 'tests-domain');
    expect(res.status).toBe(200);
    const q = r.asked[0];
    expect(q).toContain('GRAPH <urn:chorus:domains:code>');
    expect(q).toContain(`chorus:hasDomain <${TESTS}>`);
    expect(q).not.toContain('urn:chorus:instances');
    expect(q).not.toContain('hasCodeFile');
    const body = res.body as { _meta: { graph: string; count: number }; data: { files: unknown[]; tests: unknown[] } };
    expect(body._meta.graph).toBe('urn:chorus:domains:code');
    expect(body.data.files.length + body.data.tests.length).toBe(3);
    expect(body._meta.count).toBe(3);
  });
  it('a hasKind IRI renders as its kind word, not the IRI', async () => {
    const r = recorder([{ file: v('https://jeffbridwell.com/chorus#codefile-x'), filePath: v('CLAUDE.md'), fileType: v('https://jeffbridwell.com/chorus#code-kind-doc') }]);
    const res = await fetchAthenaSubdomainCode({ sparql: r.sparql as never, extname: (p) => p.slice(p.lastIndexOf('.')), now: () => 0 }, 'tests');
    const body = res.body as { data: { files: Array<{ type: string }> } };
    expect(body.data.files[0].type).toBe('doc');
  });
  it('a domain with no rows answers 0 and still names the graph it asked (so the page can say so)', async () => {
    const r = recorder([]);
    const res = await fetchAthenaSubdomainCode({ sparql: r.sparql as never, extname: () => '', now: () => 0 }, 'nothing');
    const body = res.body as { _meta: { graph: string; count: number } };
    expect(body._meta.count).toBe(0);
    expect(body._meta.graph).toBe('urn:chorus:domains:code');
  });
});

describe('#4285 API Contract fold', () => {
  it('asks the code graph for Endpoint rows by hasDomain', async () => {
    const r = recorder([
      { method: v('GET'), routePath: v('/nightly'), filePath: v('platform/api/src/server.ts') },
      { method: v('GET'), routePath: v('/api/chorus/nightly/runs'), filePath: v('platform/api/src/server.ts') },
    ]);
    const res = await fetchAthenaSubdomainEndpoints({ sparql: r.sparql as never, now: () => 0 }, 'tests');
    const q = r.asked[0];
    expect(q).toContain('GRAPH <urn:chorus:domains:code>');
    expect(q).toContain(`chorus:hasDomain <${TESTS}>`);
    expect(q).not.toContain('hasEndpoint');
    const body = res.body as { _meta: { graph: string }; data: { endpoints: Array<{ path: string }> } };
    expect(body._meta.graph).toBe('urn:chorus:domains:code');
    expect(body.data.endpoints.map((e) => e.path)).toEqual(['/nightly', '/api/chorus/nightly/runs']);
  });
});

describe('#4285 UI Pages fold', () => {
  it('asks the code graph for Page rows by hasDomain', async () => {
    const r = recorder([{ route: v('/athena/domain.html'), filePath: v('platform/api/public/athena/domain.html'), pageType: v('html') }]);
    const res = await fetchAthenaSubdomainPages({ sparql: r.sparql as never, now: () => 0 }, 'domains-domain');
    const q = r.asked[0];
    expect(q).toContain('GRAPH <urn:chorus:domains:code>');
    expect(q).toContain('chorus:hasDomain <https://jeffbridwell.com/chorus#domains>');
    expect(q).not.toContain('hasPage');
    const body = res.body as { _meta: { graph: string; count: number } };
    expect(body._meta.graph).toBe('urn:chorus:domains:code');
    expect(body._meta.count).toBe(1);
  });
});

describe('#4285 Alerts fold', () => {
  const deps = (bindings: unknown[]) => {
    const r = recorder(bindings);
    return {
      r,
      d: {
        sparql: r.sparql as never,
        resolveSubdomainId: async (n: string) => n,
        envelope: (name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) => ({ _meta: { query_name: name, duration_ms: durationMs, ...extra }, data }),
        now: () => 0,
      },
    };
  };
  it('reads Alert rows from the alerts graph by hasDomain, not YAML files by keyword', async () => {
    const { r, d } = deps([
      { name: v('GitHub Actions quality workflow on main branch is failing'), alertFile: v('chorus/proving/domains/alerts/ci-main-red.yml'), alertSource: v('script'), alertRoute: v('critical') },
      { name: v('test-nightly-notify.sh'), alertFile: v('chorus/platform/scripts/test-nightly-notify.sh'), alertSource: v('script') },
    ]);
    const res = await fetchDomainAlerts(d as never, 'tests');
    const q = r.asked[0];
    expect(q).toContain('GRAPH <urn:chorus:domains:alerts>');
    expect(q).toContain(`chorus:hasDomain <${TESTS}>`);
    const body = res.body as { _meta: { graph: string; count: number }; data: { alerts: Array<{ name: string; file: string; source: string }> } };
    expect(body._meta.graph).toBe('urn:chorus:domains:alerts');
    expect(body._meta.count).toBe(2);
    expect(body.data.alerts[1]).toEqual({ name: 'test-nightly-notify.sh', file: 'chorus/platform/scripts/test-nightly-notify.sh', source: 'script', route: '' });
  });
  it('NEGATIVE PROOF: a domain with no Alert rows answers 0 with the graph named, never a keyword match from files', async () => {
    const { d } = deps([]);
    const res = await fetchDomainAlerts(d as never, 'pulse');
    const body = res.body as { _meta: { graph: string; count: number }; data: { alerts: unknown[] } };
    expect(body.data.alerts).toEqual([]);
    expect(body._meta.count).toBe(0);
    expect(body._meta.graph).toBe('urn:chorus:domains:alerts');
  });
});
