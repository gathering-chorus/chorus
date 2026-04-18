/**
 * chorus-code-files handler — unit tests (#2188).
 */
import { fetchChorusCodeFiles, type ChorusCodeFilesDeps } from '../../src/handlers/chorus-code-files';

function sparqlResult(paths: string[]) {
  return { results: { bindings: paths.map((p) => ({ filePath: { value: p } })) } };
}

describe('fetchChorusCodeFiles (#2188)', () => {
  test('domain-suffix query returns files directly', async () => {
    let captured = '';
    const sparql: ChorusCodeFilesDeps['sparql'] = async (q) => {
      captured = q;
      return sparqlResult(['src/photos.ts', 'src/photos-util.ts']);
    };
    const r = await fetchChorusCodeFiles({ sparql }, 'photos');
    expect(r.status).toBe(200);
    const body = r.body as { files: string[]; count: number; domain: string };
    expect(body.files).toEqual(['src/photos.ts', 'src/photos-util.ts']);
    expect(body.count).toBe(2);
    expect(body.domain).toBe('photos');
    expect(captured).toContain('photos-domain');
  });

  test('falls back to -service when -domain returns empty', async () => {
    const queries: string[] = [];
    const sparql: ChorusCodeFilesDeps['sparql'] = async (q) => {
      queries.push(q);
      if (q.includes('photos-domain')) return sparqlResult([]);
      if (q.includes('photos-service')) return sparqlResult(['src/svc.ts']);
      return sparqlResult([]);
    };
    const r = await fetchChorusCodeFiles({ sparql }, 'photos');
    const body = r.body as { files: string[] };
    expect(body.files).toEqual(['src/svc.ts']);
    expect(queries.length).toBe(2);
  });

  test('already-suffixed -service name skips fallback', async () => {
    let calls = 0;
    const sparql: ChorusCodeFilesDeps['sparql'] = async () => { calls++; return sparqlResult([]); };
    await fetchChorusCodeFiles({ sparql }, 'photos-service');
    expect(calls).toBe(1);
  });

  test('already-suffixed -domain name skips fallback', async () => {
    let calls = 0;
    const sparql: ChorusCodeFilesDeps['sparql'] = async () => { calls++; return sparqlResult([]); };
    await fetchChorusCodeFiles({ sparql }, 'photos-domain');
    expect(calls).toBe(1);
  });

  test('sparql throws → empty files, 200', async () => {
    const sparql: ChorusCodeFilesDeps['sparql'] = async () => { throw new Error('fuseki down'); };
    const r = await fetchChorusCodeFiles({ sparql }, 'photos');
    expect(r.status).toBe(200);
    const body = r.body as { files: unknown[]; count: number };
    expect(body.files).toEqual([]);
    expect(body.count).toBe(0);
  });

  test('uppercase domain lowercased in response', async () => {
    const sparql: ChorusCodeFilesDeps['sparql'] = async () => sparqlResult([]);
    const body = (await fetchChorusCodeFiles({ sparql }, 'PHOTOS')).body as { domain: string };
    expect(body.domain).toBe('photos');
  });
});
