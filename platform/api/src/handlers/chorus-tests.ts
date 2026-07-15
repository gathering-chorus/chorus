/**
 * GET /api/chorus/tests, GET /api/chorus/tests/:domain (#2098, extracted #2189).
 *
 * #3656: rewired from HTTP proxies against Gathering's /api/quality/* (retired
 * with the quality-service residue) to direct calls into the local scanner
 * (quality-summary.ts). Response envelopes are unchanged:
 *
 *   /api/chorus/tests         → local scan, flatten pyramid.files
 *   /api/chorus/tests/:domain → local per-domain scan
 *
 * Scanner throws → 500 with error envelope (there is no upstream to pass
 * through anymore).
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
    files?: Array<{ name?: string; kind?: string; domain?: string; count?: number }>;
  }>;
}

export interface DomainScanLike {
  total?: number;
}

export interface TestsDeps {
  envelope: EnvelopeFn;
  now?: () => number;
  scanFn?: () => ScanLike;
  byDomainFn?: (domain: string) => DomainScanLike;
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
    const data = byDomainFn(lower);
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
    const data = scanFn();
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
      status: 500,
      body: envelope('quality-scan', { error: (err as Error).message }, now() - start, { error: true }),
    };
  }
}
