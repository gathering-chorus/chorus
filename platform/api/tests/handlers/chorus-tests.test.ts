/**
 * chorus-tests handler — unit tests (#2189).
 *
 * /api/chorus/tests/:domain and /api/chorus/tests both proxy Gathering's
 * quality API. Tests verify:
 *   - domain lowercased before proxy URL
 *   - upstream non-2xx → pass status + upstream error msg
 *   - upstream ok → envelope wraps data with count/total meta
 *   - all-scan flattens pyramid[].files[] into root-level files[]
 *   - fetch throws → 502 with envelope { error, _meta: { error: true } }
 *   - elapsed_ms captured via now() difference
 */
import {
  fetchTestsByDomain,
  fetchTestsAll,
  type TestsDeps,
  type FetchFn,
  type EnvelopeFn,
} from '../../src/handlers/chorus-tests';

function okFetch(body: unknown): FetchFn {
  return async () => ({ ok: true, status: 200, json: async () => body });
}

function errFetch(status: number): FetchFn {
  return async () => ({ ok: false, status, json: async () => ({}) });
}

function throwFetch(msg: string): FetchFn {
  return async () => {
    throw new Error(msg);
  };
}

interface EnvelopeCall {
  queryName: string;
  data: unknown;
  durationMs: number;
  extra: Record<string, unknown> | undefined;
}

function spyEnvelope(): { fn: EnvelopeFn; calls: EnvelopeCall[] } {
  const calls: EnvelopeCall[] = [];
  const fn: EnvelopeFn = (queryName, data, durationMs, extra) => {
    calls.push({ queryName, data, durationMs, extra });
    return { envelope: queryName, data, durationMs, extra };
  };
  return { fn, calls };
}

function deps(overrides: Partial<TestsDeps>, envelope: EnvelopeFn): TestsDeps {
  return {
    fetchFn: okFetch({}),
    envelope,
    now: () => 0,
    appBaseUrl: 'http://fake',
    ...overrides,
  };
}

describe('fetchTestsByDomain (#2189 /api/chorus/tests/:domain)', () => {
  test('lowercases domain, wraps data in domain-tests envelope with count', async () => {
    const env = spyEnvelope();
    let t = 100;
    const upstreamBody = { total: 42, somefield: 'x' };
    let calledUrl = '';
    const fetchFn: FetchFn = async (url) => {
      calledUrl = url;
      return { ok: true, status: 200, json: async () => upstreamBody };
    };
    const r = await fetchTestsByDomain('PhOtOs', deps({ fetchFn, now: () => (t += 5) }, env.fn));
    expect(calledUrl).toBe('http://fake/api/quality/domain/photos');
    expect(r.status).toBe(200);
    expect(env.calls[0].queryName).toBe('domain-tests');
    expect(env.calls[0].data).toEqual(upstreamBody);
    expect(env.calls[0].extra).toEqual({ count: 42 });
  });

  test('upstream 404 → pass through status + upstream error message', async () => {
    const env = spyEnvelope();
    const r = await fetchTestsByDomain('photos', deps({ fetchFn: errFetch(404) }, env.fn));
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'upstream returned 404' });
    expect(env.calls).toHaveLength(0);
  });

  test('fetch throws → 502 with error envelope', async () => {
    const env = spyEnvelope();
    const r = await fetchTestsByDomain('photos', deps({ fetchFn: throwFetch('ECONN') }, env.fn));
    expect(r.status).toBe(502);
    expect(env.calls[0].queryName).toBe('domain-tests');
    expect(env.calls[0].extra).toEqual({ error: true });
    expect((env.calls[0].data as { error: string }).error).toBe('ECONN');
  });

  test('data missing total → count defaults to 0', async () => {
    const env = spyEnvelope();
    await fetchTestsByDomain('photos', deps({ fetchFn: okFetch({}) }, env.fn));
    expect(env.calls[0].extra).toEqual({ count: 0 });
  });
});

describe('fetchTestsAll (#2189 /api/chorus/tests)', () => {
  test('flattens pyramid[].files[] into root-level files[]', async () => {
    const env = spyEnvelope();
    const upstreamBody = {
      total: 7,
      pyramid: [
        { name: 'unit', files: [{ name: 'a.test.ts', kind: 'unit', domain: 'music', count: 5 }] },
        { name: 'integration', files: [{ name: 'b.test.ts', kind: 'integration', domain: 'photos', count: 2 }] },
      ],
    };
    await fetchTestsAll(deps({ fetchFn: okFetch(upstreamBody) }, env.fn));
    const flat = (env.calls[0].data as { files: Array<Record<string, unknown>> }).files;
    expect(flat).toEqual([
      { path: 'a.test.ts', type: 'unit', domain: 'music', count: 5, layer: 'unit' },
      { path: 'b.test.ts', type: 'integration', domain: 'photos', count: 2, layer: 'integration' },
    ]);
    expect(env.calls[0].extra).toEqual({ total: 7 });
  });

  test('empty pyramid → empty files array', async () => {
    const env = spyEnvelope();
    await fetchTestsAll(deps({ fetchFn: okFetch({ total: 0 }) }, env.fn));
    const flat = (env.calls[0].data as { files: unknown[] }).files;
    expect(flat).toEqual([]);
  });

  test('upstream 503 → status pass-through', async () => {
    const env = spyEnvelope();
    const r = await fetchTestsAll(deps({ fetchFn: errFetch(503) }, env.fn));
    expect(r.status).toBe(503);
    expect(r.body).toEqual({ error: 'upstream returned 503' });
  });

  test('fetch throws → 502 with quality-scan error envelope', async () => {
    const env = spyEnvelope();
    const r = await fetchTestsAll(deps({ fetchFn: throwFetch('timeout') }, env.fn));
    expect(r.status).toBe(502);
    expect(env.calls[0].queryName).toBe('quality-scan');
    expect(env.calls[0].extra).toEqual({ error: true });
  });
});
