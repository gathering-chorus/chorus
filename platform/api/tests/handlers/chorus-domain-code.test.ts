// @test-type: unit — stubs the store; no Fuseki, no live service.
// @card: #2188
// @owner: wren
/**
 * chorus-domain-code handler — unit tests (#2188).
 */
import { fetchChorusDomainCode, isTestFile, type ChorusDomainCodeDeps } from '../../src/handlers/chorus-domain-code';

const envelope = (queryName: string, data: unknown, _d: number, extra?: Record<string, unknown>) => ({
  _meta: { query_name: queryName, ...extra }, data,
});

function sparqlResult(rows: Array<{ file: string; label?: string; filePath?: string; fileType?: string; description?: string }>) {
  return {
    results: {
      bindings: rows.map((r) => ({
        file: { value: r.file },
        ...(r.label ? { label: { value: r.label } } : {}),
        ...(r.filePath ? { filePath: { value: r.filePath } } : {}),
        ...(r.fileType ? { fileType: { value: r.fileType } } : {}),
        ...(r.description ? { description: { value: r.description } } : {}),
      })),
    },
  };
}

function deps(over: Partial<ChorusDomainCodeDeps> = {}): ChorusDomainCodeDeps {
  return {
    sparql: async () => sparqlResult([]),
    envelope,
    now: () => 1_000_000,
    ...over,
  };
}

describe('isTestFile', () => {
  test('matches /tests/ and /__tests__/', () => {
    expect(isTestFile('src/tests/foo.ts')).toBe(true);
    expect(isTestFile('src/__tests__/bar.ts')).toBe(true);
  });
  test('matches .test. and .spec.', () => {
    expect(isTestFile('foo.test.ts')).toBe(true);
    expect(isTestFile('foo.spec.tsx')).toBe(true);
  });
  test('matches .bats, _test.rs, .feature', () => {
    expect(isTestFile('x.bats')).toBe(true);
    expect(isTestFile('foo_test.rs')).toBe(true);
    expect(isTestFile('scenarios.feature')).toBe(true);
  });
  test('does NOT match normal source paths', () => {
    expect(isTestFile('src/foo.ts')).toBe(false);
    expect(isTestFile('lib/index.js')).toBe(false);
  });
});

describe('fetchChorusDomainCode (#2188)', () => {
  test('empty sparql → envelope with empty files + byType', async () => {
    const r = await fetchChorusDomainCode(deps(), 'photos');
    const body = r.body as { _meta: { count: number }; data: { files: unknown[]; byType: Record<string, number> } };
    expect(body._meta.count).toBe(0);
    expect(body.data.files).toEqual([]);
    expect(body.data.byType).toEqual({});
  });

  test('filePath used as path; fileType preferred', async () => {
    const sparql = async () => sparqlResult([
      { file: 'x#foo', filePath: 'src/foo.ts', fileType: 'typescript' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      data: { files: Array<{ path: string; type: string }> };
    };
    expect(body.data.files[0].path).toBe('src/foo.ts');
    expect(body.data.files[0].type).toBe('typescript');
  });

  test('fileType absent → falls back to extname of filePath', async () => {
    const sparql = async () => sparqlResult([
      { file: 'x#foo', filePath: 'src/foo.ts' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      data: { files: Array<{ type: string }> };
    };
    expect(body.data.files[0].type).toBe('ts');
  });

  test('label used when filePath missing; type falls to "unknown"', async () => {
    const sparql = async () => sparqlResult([
      { file: 'x#foo', label: 'FooLabel' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      data: { files: Array<{ path: string; type: string }> };
    };
    expect(body.data.files[0].path).toBe('FooLabel');
    expect(body.data.files[0].type).toBe('unknown');
  });

  test('file URI last-segment used when filePath+label both missing', async () => {
    const sparql = async () => sparqlResult([
      { file: 'https://x#the-file-id' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      data: { files: Array<{ path: string }> };
    };
    expect(body.data.files[0].path).toBe('the-file-id');
  });

  test('excludes test files from source but counts them', async () => {
    const sparql = async () => sparqlResult([
      { file: 'x#a', filePath: 'src/foo.ts' },
      { file: 'x#b', filePath: 'tests/foo.test.ts' },
      { file: 'x#c', filePath: 'src/bar.ts' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      _meta: { count: number; source_count: number; test_count: number };
      data: { files: Array<{ path: string }> };
    };
    expect(body._meta.count).toBe(3);
    expect(body._meta.source_count).toBe(2);
    expect(body._meta.test_count).toBe(1);
    expect(body.data.files.map((f) => f.path)).toEqual(['src/foo.ts', 'src/bar.ts']);
  });

  test('byType aggregates on source files only', async () => {
    const sparql = async () => sparqlResult([
      { file: 'x#a', filePath: 'src/foo.ts', fileType: 'typescript' },
      { file: 'x#b', filePath: 'src/bar.ts', fileType: 'typescript' },
      { file: 'x#c', filePath: 'src/baz.rs', fileType: 'rust' },
      { file: 'x#d', filePath: 'tests/foo.test.ts', fileType: 'typescript' },
    ]);
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'photos')).body as {
      data: { byType: Record<string, number> };
    };
    expect(body.data.byType).toEqual({ typescript: 2, rust: 1 });
  });

  test('#4353 — asks for the domain by its own name and answers with domain, never a guessed -domain id', async () => {
    const asked: string[] = [];
    const sparql = async (q: string) => { asked.push(q); return sparqlResult([]); };
    const body = (await fetchChorusDomainCode(deps({ sparql }), 'Photos')).body as { data: Record<string, unknown> };
    expect(asked).toHaveLength(1);
    expect(asked[0]).toContain('<https://jeffbridwell.com/chorus#photos>');
    expect(asked[0]).not.toContain('photos-domain');
    expect(body.data.domain).toBe('photos');
    expect(Object.keys(body.data).sort()).toEqual(['byType', 'domain', 'files']);
  });

  test('sparql throws → empty envelope', async () => {
    const body = (await fetchChorusDomainCode(
      deps({ sparql: async () => { throw new Error('fuseki down'); } }),
      'photos',
    )).body as { _meta: { count: number } };
    expect(body._meta.count).toBe(0);
  });
});
