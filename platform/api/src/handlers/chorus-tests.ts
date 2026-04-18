/**
 * GET /api/chorus/tests, GET /api/chorus/tests/:domain (#2098, extracted #2189).
 *
 * Thin proxies to Gathering's quality scanner at port 3000. Both return
 * athenaEnvelope-shaped responses (envelope injected for testability).
 *
 *   /api/chorus/tests         → proxy to /api/quality/scan, flatten pyramid.files
 *   /api/chorus/tests/:domain → proxy to /api/quality/domain/<domain>
 *
 * Upstream non-2xx → pass status through with minimal error. fetch throws → 502.
 */

export type FetchFn = (
  url: string,
  init?: { signal?: AbortSignal },
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>;

export type EnvelopeFn = (
  queryName: string,
  data: unknown,
  durationMs: number,
  extra?: Record<string, unknown>,
) => unknown;

export interface TestsDeps {
  fetchFn?: FetchFn;
  envelope: EnvelopeFn;
  now?: () => number;
  appBaseUrl?: string;
}

export interface TestsResult {
  status: number;
  body: unknown;
}

export async function fetchTestsByDomain(
  domain: string,
  { fetchFn = globalThis.fetch as FetchFn, envelope, now = Date.now, appBaseUrl = 'http://localhost:3000' }: TestsDeps,
): Promise<TestsResult> {
  const start = now();
  const lower = (domain || '').toLowerCase();
  try {
    const upstream = await fetchFn(`${appBaseUrl}/api/quality/domain/${lower}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!upstream.ok) {
      return { status: upstream.status, body: { error: `upstream returned ${upstream.status}` } };
    }
    const data = (await upstream.json()) as { total?: number };
    return {
      status: 200,
      body: envelope('domain-tests', data, now() - start, { count: data.total || 0 }),
    };
  } catch (err) {
    return {
      status: 502,
      body: envelope('domain-tests', { error: (err as Error).message }, now() - start, { error: true }),
    };
  }
}

export async function fetchTestsAll({
  fetchFn = globalThis.fetch as FetchFn,
  envelope,
  now = Date.now,
  appBaseUrl = 'http://localhost:3000',
}: TestsDeps): Promise<TestsResult> {
  const start = now();
  try {
    const upstream = await fetchFn(`${appBaseUrl}/api/quality/scan`, {
      signal: AbortSignal.timeout(10000),
    });
    if (!upstream.ok) {
      return { status: upstream.status, body: { error: `upstream returned ${upstream.status}` } };
    }
    const data = (await upstream.json()) as {
      pyramid?: Array<{ name?: string; files?: Array<{ name?: string; kind?: string; domain?: string; count?: number }> }>;
      total?: number;
    };
    const allFiles = (data.pyramid || []).flatMap((l) =>
      (l.files || []).map((f) => ({
        path: f.name,
        type: f.kind,
        domain: f.domain,
        count: f.count,
        layer: l.name,
      })),
    );
    const enriched = { ...data, files: allFiles };
    return {
      status: 200,
      body: envelope('quality-scan', enriched, now() - start, { total: data.total || 0 }),
    };
  } catch (err) {
    return {
      status: 502,
      body: envelope('quality-scan', { error: (err as Error).message }, now() - start, { error: true }),
    };
  }
}
