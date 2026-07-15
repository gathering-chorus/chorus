/**
 * GET /api/chorus/tests, GET /api/chorus/tests/:domain (#2098, extracted #2189).
 *
 * #3657: quality-summary.ts is now a projection of the tests domain (owl-api
 * V2 /tests collection), async, and carries no api/ui kind — that was a
 * scanner heuristic the model doesn't declare. The flattened file rows carry
 * the pyramid layer as `layer`. Response envelopes otherwise unchanged:
 *
 *   /api/chorus/tests         → domain projection, flatten pyramid.files
 *   /api/chorus/tests/:domain → per-domain filter on the model's `covers`
 *
 * Projection throws (owl-api unreachable) → 500 with error envelope.
 */

import { getQualityScan, getQualityByDomain } from '../quality-summary';

export type EnvelopeFn = (
  queryName: string,
  data: unknown,
  durationMs: number,
  extra?: Record<string, unknown>,
) => unknown;

// Structural shapes the handler actually consumes — injectable for tests.
export interface ScanLike {
  total?: number;
  pyramid?: Array<{
    name?: string;
    files?: Array<{ name?: string; domain?: string; count?: number }>;
  }>;
}

export interface DomainScanLike {
  total?: number;
}

export interface TestsDeps {
  envelope: EnvelopeFn;
  now?: () => number;
  scanFn?: () => ScanLike | Promise<ScanLike>;
  byDomainFn?: (domain: string) => DomainScanLike | Promise<DomainScanLike>;
}

export interface TestsResult {
  status: number;
  body: unknown;
}

export async function fetchTestsByDomain(
  domain: string,
  { envelope, now = Date.now, byDomainFn = getQualityByDomain }: TestsDeps,
): Promise<TestsResult> {
  const start = now();
  const lower = (domain || '').toLowerCase();
  try {
    const data = await byDomainFn(lower);
    return {
      status: 200,
      body: envelope('domain-tests', data, now() - start, { count: data.total || 0 }),
    };
  } catch (err) {
    return {
      status: 500,
      body: envelope('domain-tests', { error: (err as Error).message }, now() - start, { error: true }),
    };
  }
}

export async function fetchTestsAll({
  envelope,
  now = Date.now,
  scanFn = getQualityScan,
}: TestsDeps): Promise<TestsResult> {
  const start = now();
  try {
    const data = await scanFn();
    const allFiles = (data.pyramid || []).flatMap((l) =>
      (l.files || []).map((f) => ({
        path: f.name,
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
      status: 500,
      body: envelope('quality-scan', { error: (err as Error).message }, now() - start, { error: true }),
    };
  }
}
