/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Indexing on SOURCE_CADENCE keys (validated source enum); no user-controlled fs.
 */
// Search freshness metadata (extracted from server.ts for #2205 wave 5).
// - SOURCE_CADENCE: per-source expected freshness bands.
// - STALE_THRESHOLD_MS: global stale cutoff for the watermark-latest check.
// - addStaleHeader: X-Chorus-Stale response header when the index is behind.
// - buildSearchMeta: freshness + coverage block that ships with every search
//   response (#1878 / #1879 / #2174).

import type { Response } from 'express';
import type Database from 'better-sqlite3';

export const STALE_THRESHOLD_MS = 60 * 60 * 1000; // 1 hour

export const SOURCE_CADENCE: Record<string, number> = {
  claude: 3600,
  spine: 3600,
  brief: 86400,
  decision: 86400,
  clearing: 86400,
  memory: 86400,
  story: 86400,
  adr: 604800,
  activity: 86400,
  state: 86400,
  crawler: 86400,
  journal: 604800,
};

/**
 * Add the X-Chorus-Stale header when the most recent watermark is older than
 * STALE_THRESHOLD_MS. Silent no-op when there are no watermarks.
 */
export function addStaleHeader(res: Response, db: Database.Database): void {
  const row = db.prepare(
    'SELECT MAX(last_indexed) as latest FROM watermarks',
  ).get() as { latest: string } | undefined;

  if (row?.latest) {
    const lastIndexed = new Date(row.latest).getTime();
    if (Date.now() - lastIndexed > STALE_THRESHOLD_MS) {
      res.setHeader('X-Chorus-Stale', 'true');
    }
  }
}

/**
 * Build the freshness + coverage meta block shipped with every search
 * response. Defensively handles missing db, broken db queries, missing
 * timestamps, unknown sources.
 */
interface SearchMetaResult {
  timestamp?: string;
  source?: string;
  domain?: string;
}

function aggregateWatermarks(db: Database.Database): Map<string, string> {
  const watermarks = db.prepare('SELECT source, last_indexed FROM watermarks ORDER BY source')
    .all() as Array<{ source: string; last_indexed: string }>;
  const aggregated = new Map<string, string>();
  for (const w of watermarks) {
    const parts = w.source.split(':');
    const key = parts[0] === 'artifact' ? parts.slice(0, 2).join(':') : parts[0];
    const existing = aggregated.get(key);
    if (!existing || w.last_indexed > existing) aggregated.set(key, w.last_indexed);
  }
  return aggregated;
}

function computeDomainCoverage(db?: Database.Database): number {
  if (!db) return 1;
  try {
    const aggregated = aggregateWatermarks(db);
    const now = Date.now();
    let total = 0, contributing = 0;
    for (const [source, lastIndexed] of aggregated) {
      total++;
      const ageSecs = (now - new Date(lastIndexed).getTime()) / 1000;
      const cadenceKey = source.split(':')[0];
      const cadence = SOURCE_CADENCE[cadenceKey] || SOURCE_CADENCE[source] || 86400;
      if (ageSecs / cadence <= 2) contributing++;
    }
    return total > 0 ? contributing / total : 1;
  } catch {
    return 1;
  }
}

function computeNewestResultAge(results: SearchMetaResult[]): number {
  if (results.length === 0) return 0;
  const timestamps = results
    .map((r) => r.timestamp)
    .filter((t): t is string => Boolean(t))
    .map((t) => new Date(t).getTime())
    .filter((t) => !isNaN(t));
  if (timestamps.length === 0) return 0;
  return Math.round((Date.now() - Math.max(...timestamps)) / 1000);
}

function tallySources(results: SearchMetaResult[]): Record<string, number> {
  const sources: Record<string, number> = {};
  for (const r of results) {
    const src = r.source || r.domain || 'unknown';
    sources[src] = (sources[src] || 0) + 1;
  }
  return sources;
}

export function buildSearchMeta(results: SearchMetaResult[], db?: Database.Database): Record<string, unknown> {
  const domain_coverage = computeDomainCoverage(db);
  const newest_result_age_s = computeNewestResultAge(results);
  const stale = newest_result_age_s > 86400 || domain_coverage < 0.5;
  return {
    domain_coverage: Math.round(domain_coverage * 100) / 100,
    newest_result_age_s,
    stale,
    sources: tallySources(results),
    schema_version: '1.0.0',
  };
}
