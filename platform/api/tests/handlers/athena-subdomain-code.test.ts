/**
 * athena-subdomain-code handler — unit tests (#2187).
 *
 * Returns code files attached to a sub-domain via chorus:hasCodeFile.
 * Splits files into source vs tests by path pattern; groups byType.
 */
import {
  fetchAthenaSubdomainCode,
  type AthenaSubdomainCodeDeps,
  type SparqlCodeBinding,
} from '../../src/handlers/athena-subdomain-code';

function result(bindings: SparqlCodeBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaSubdomainCodeDeps> = {}): AthenaSubdomainCodeDeps {
  return {
    sparql: async () => result([]),
    extname: (p: string) => {
      const i = p.lastIndexOf('.');
      return i === -1 ? '' : p.slice(i);
    },
    now: () => 1_000_000,
    ...overrides,
  };
}

function file(path: string, type?: string): SparqlCodeBinding {
  const b: SparqlCodeBinding = { file: { value: `https://jeffbridwell.com/chorus#${path.replace(/[\/.]/g, '-')}` } };
  b.filePath = { value: path };
  if (type) b.fileType = { value: type };
  return b;
}

describe('fetchAthenaSubdomainCode (#2187)', () => {
  test('empty result returns 200 with empty arrays', async () => {
    const r = await fetchAthenaSubdomainCode(deps(), 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { files: Array<unknown>; tests: Array<unknown>; byType: Record<string, number> } };
    expect(body.data.files).toEqual([]);
    expect(body.data.tests).toEqual([]);
    expect(body.data.byType).toEqual({});
  });

  test('path in /tests/ lands in tests[] not files[]', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([file('src/handlers/foo.test.ts', 'ts'), file('tests/bar.test.ts', 'ts')]),
    }), 'x');
    const body = r.body as { data: { files: Array<{ path: string }>; tests: Array<{ path: string }> } };
    expect(body.data.tests.map((f) => f.path)).toEqual(['src/handlers/foo.test.ts', 'tests/bar.test.ts']);
    expect(body.data.files).toEqual([]);
  });

  test('.spec and _test.rs and .bats and .feature are classified as tests', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([
        file('src/foo.spec.ts'),
        file('crates/x/src/foo_test.rs'),
        file('tests/smoke.bats'),
        file('features/auth.feature'),
      ]),
    }), 'x');
    const body = r.body as { data: { tests: Array<unknown>; files: Array<unknown> } };
    expect(body.data.tests).toHaveLength(4);
    expect(body.data.files).toHaveLength(0);
  });

  test('non-test paths land in files[]', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([file('src/handlers/athena-products.ts', 'ts'), file('README.md', 'md')]),
    }), 'x');
    const body = r.body as { data: { files: Array<{ path: string }>; tests: Array<unknown> } };
    expect(body.data.files.map((f) => f.path)).toEqual(['src/handlers/athena-products.ts', 'README.md']);
    expect(body.data.tests).toEqual([]);
  });

  test('byType counts every file including tests', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([
        file('a.ts', 'ts'),
        file('b.ts', 'ts'),
        file('c.test.ts', 'ts'),
        file('d.md', 'md'),
      ]),
    }), 'x');
    const body = r.body as { data: { byType: Record<string, number> } };
    expect(body.data.byType).toEqual({ ts: 3, md: 1 });
  });

  test('missing fileType falls back to extname of filePath (no leading dot)', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([{
        file: { value: '#f' },
        filePath: { value: 'handlers/foo.ts' },
      }]),
    }), 'x');
    const body = r.body as { data: { files: Array<{ type: string }> } };
    expect(body.data.files[0].type).toBe('ts');
  });

  test('missing filePath and fileType: type is "unknown"', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([{
        file: { value: 'https://jeffbridwell.com/chorus#bare' },
      }]),
    }), 'x');
    const body = r.body as { data: { files: Array<{ type: string; path: string }> } };
    expect(body.data.files[0].type).toBe('unknown');
    expect(body.data.files[0].path).toBe('bare');
  });

  test('meta counts source + tests separately', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => result([
        file('src/a.ts'),
        file('src/b.ts'),
        file('src/c.test.ts'),
      ]),
    }), 'x');
    const body = r.body as { _meta: { count: number; source_count: number; test_count: number } };
    expect(body._meta.count).toBe(3);
    expect(body._meta.source_count).toBe(2);
    expect(body._meta.test_count).toBe(1);
  });

  test('SPARQL throws returns 500', async () => {
    const r = await fetchAthenaSubdomainCode(deps({
      sparql: async () => { throw new Error('down'); },
    }), 'x');
    expect(r.status).toBe(500);
    const body = r.body as { _meta: { error: boolean } };
    expect(body._meta.error).toBe(true);
  });
});
