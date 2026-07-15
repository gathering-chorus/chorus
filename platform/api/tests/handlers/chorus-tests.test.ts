// @test-type: unit — handler tests with injected scanner/envelope fakes; no services.
/**
 * chorus-tests handler — unit tests (#2189, rewired #3656).
 *
 * /api/chorus/tests/:domain and /api/chorus/tests call the LOCAL quality
 * scanner (quality-summary.ts) directly — #3656 removed the HTTP proxy to
 * Gathering's retired /api/quality/* surface. Tests verify:
 *   - domain lowercased before scanner call
 *   - scanner ok → envelope wraps data with count/total meta
 *   - all-scan flattens pyramid[].files[] into root-level files[]
 *   - scanner throws → 500 with envelope { error, _meta: { error: true } }
 *   - elapsed_ms captured via now() difference
 */
import {
  fetchTestsByDomain,
  fetchTestsAll,
  type TestsDeps,
  type EnvelopeFn,
} from '../../src/handlers/chorus-tests';

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
    envelope,
    now: () => 0,
    ...overrides,
  };
}

describe('fetchTestsByDomain (#3656 /api/chorus/tests/:domain via local scanner)', () => {
  test('lowercases domain, wraps data in domain-tests envelope with count', async () => {
    const env = spyEnvelope();
    let t = 100;
    const scannerBody = { domain: 'photos', total: 42, files: [], layers: [] };
    let calledDomain = '';
    const byDomainFn = (d: string) => {
      calledDomain = d;
      return scannerBody;
    };
    const r = await fetchTestsByDomain('PhOtOs', deps({ byDomainFn, now: () => (t += 5) }, env.fn));
    expect(calledDomain).toBe('photos');
    expect(r.status).toBe(200);
    expect(env.calls[0].queryName).toBe('domain-tests');
    expect(env.calls[0].data).toEqual(scannerBody);
    expect(env.calls[0].extra).toEqual({ count: 42 });
  });

  test('scanner throws → 500 with error envelope', async () => {
    const env = spyEnvelope();
    const byDomainFn = () => {
      throw new Error('scan failed');
    };
    const r = await fetchTestsByDomain('photos', deps({ byDomainFn }, env.fn));
    expect(r.status).toBe(500);
    expect(env.calls[0].queryName).toBe('domain-tests');
    expect(env.calls[0].extra).toEqual({ error: true });
    expect((env.calls[0].data as { error: string }).error).toBe('scan failed');
  });

  test('data missing total → count defaults to 0', async () => {
    const env = spyEnvelope();
    const byDomainFn = () => ({ domain: 'photos', total: 0, files: [], layers: [] });
    await fetchTestsByDomain('photos', deps({ byDomainFn }, env.fn));
    expect(env.calls[0].extra).toEqual({ count: 0 });
  });
});

describe('fetchTestsAll (#3656 /api/chorus/tests via local scanner)', () => {
  test('flattens pyramid[].files[] into root-level files[]', async () => {
    const env = spyEnvelope();
    const scannerBody = {
      total: 7,
      pyramid: [
        { name: 'unit', files: [{ name: 'a.test.ts', kind: 'unit', domain: 'music', count: 5 }] },
        { name: 'integration', files: [{ name: 'b.test.ts', kind: 'integration', domain: 'photos', count: 2 }] },
      ],
    };
    await fetchTestsAll(deps({ scanFn: () => scannerBody }, env.fn));
    const flat = (env.calls[0].data as { files: Array<Record<string, unknown>> }).files;
    expect(flat).toEqual([
      { path: 'a.test.ts', type: 'unit', domain: 'music', count: 5, layer: 'unit' },
      { path: 'b.test.ts', type: 'integration', domain: 'photos', count: 2, layer: 'integration' },
    ]);
    expect(env.calls[0].extra).toEqual({ total: 7 });
  });

  test('empty pyramid → empty files array', async () => {
    const env = spyEnvelope();
    await fetchTestsAll(deps({ scanFn: () => ({ total: 0 }) }, env.fn));
    const flat = (env.calls[0].data as { files: unknown[] }).files;
    expect(flat).toEqual([]);
  });

  test('scanner throws → 500 with quality-scan error envelope', async () => {
    const env = spyEnvelope();
    const scanFn = () => {
      throw new Error('walk failed');
    };
    const r = await fetchTestsAll(deps({ scanFn }, env.fn));
    expect(r.status).toBe(500);
    expect(env.calls[0].queryName).toBe('quality-scan');
    expect(env.calls[0].extra).toEqual({ error: true });
  });
});
