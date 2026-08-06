/* eslint-disable security/detect-object-injection --
 * Indexing on validated typed enum keys; no user-controlled fs paths.
 */
/**
 * Interaction Patterns — #2099 per-page migration from Gathering.
 *
 * Queries Loki (localhost:3102) for interaction.pattern.detected events
 * emitted by Wren when classifying Jeff's input. Aggregates by pattern
 * (direction/ideation/demo/triage/swat/gemba/clearing/story/reflection)
 * and by date. Same shape as Gathering's /api/interaction-patterns —
 * consumers can move over without payload changes.
 */

const LOKI_URL = process.env.LOKI_URL || 'http://127.0.0.1:3102';

export interface PatternsSummaryResponse {
  patterns: Record<string, number>;
  byDate: Array<{ date: string; total: number; counts: Record<string, number> }>;
  total: number;
  days: number;
  /**
   * #3711 — WHERE this answer came from, so a caller can tell a fact from a
   * failure. Previously a Loki timeout returned {patterns:{}, total:0} with
   * HTTP 200 — indistinguishable from "there are genuinely no patterns", which
   * is a completely different statement about the world. The borg-health probe
   * reported that empty payload as a data outage from 2026-07-06 onward and it
   * was dismissed as alarm-battery noise for a month.
   *   'loki'        — Loki answered; counts are current (zero really means zero)
   *   'cache'       — served from the TTL cache; `stale` says a refresh failed
   *   'unavailable' — Loki could not be reached or read; the counts mean NOTHING
   */
  source: 'loki' | 'cache' | 'unavailable';
  /** True when serving a last-good value because a refresh failed. */
  stale?: boolean;
  /** Why the answer is unavailable (timeout, HTTP status, transport error). */
  reason?: string;
}

/**
 * #3711 — TTL cache. The Loki query genuinely costs ~3.5-3.9s (measured
 * 2026-07-29) against a 5s timeout, so it ran at ~80% of its own budget and
 * tipped whenever chorus-api was busy: five consecutive live calls went
 * 500/0/0/0/0, each failure landing on exactly 5.00s. Paying that once per
 * window rather than once per request removes the flakiness AND takes a
 * multi-second fetch off the per-request serving path. Same discipline as the
 * TTL'd Principal allow-set in chorus-oidc.
 */
const CACHE_TTL_MS = 60_000;
const LOKI_TIMEOUT_MS = 12_000;   // well clear of the ~4s real cost, now amortized
type CacheEntry = { at: number; value: PatternsSummaryResponse };
const cache = new Map<number, CacheEntry>();

/**
 * Test seam. `keepValue` retains the cached VALUE but expires it, so a test can
 * exercise the refresh-failed-serve-last-good path without waiting out the TTL.
 */
export function resetPatternsCache(opts?: { keepValue?: boolean }): void {
  if (opts?.keepValue) {
    for (const [k, v] of cache) cache.set(k, { at: 0, value: v.value });
    return;
  }
  cache.clear();
}

function recordPatternLine(line: string, patterns: Record<string, number>, byDate: Partial<Record<string, Record<string, number>>>): void {
  try {
    const obj = JSON.parse(line);
    const pattern = obj.pattern || 'unknown';
    const date = (obj.timestamp || '').slice(0, 10);
    patterns[pattern] = (patterns[pattern] || 0) + 1;
    if (date) {
      let bucket = byDate[date];
      if (!bucket) { bucket = {}; byDate[date] = bucket; }
      bucket[pattern] = (bucket[pattern] || 0) + 1;
    }
  } catch { /* skip malformed */ }
}

export async function getPatternsSummary(days: number): Promise<PatternsSummaryResponse> {
  // Serve a fresh cache entry without touching Loki at all (#3711).
  const hit = cache.get(days);
  if (hit && Date.now() - hit.at < CACHE_TTL_MS) {
    return { ...hit.value, source: 'cache', stale: false };
  }

  const start = Math.floor(Date.now() / 1000 - days * 86400);
  const end = Math.floor(Date.now() / 1000);
  const params = new URLSearchParams({
    query: '{appName="chorus-events"} | json | event="interaction.pattern.detected"',
    start: `${start}000000000`,
    end: `${end}000000000`,
    limit: '500',
  });

  const patterns: Record<string, number> = {};
  const byDate: Partial<Record<string, Record<string, number>>> = {};

  // #3711 — a refresh that fails must NEVER become "zero patterns". Serve the
  // last good value marked stale if we have one; otherwise say unavailable and
  // say why. The old code's `catch { }` did neither, and the empty payload it
  // produced read as fact to every consumer including the health probe.
  const degraded = (reason: string): PatternsSummaryResponse => {
    const prev = cache.get(days);
    if (prev) return { ...prev.value, source: 'cache', stale: true, reason };
    return { patterns: {}, byDate: [], total: 0, days, source: 'unavailable', reason };
  };

  try {
    const r = await fetch(`${LOKI_URL}/loki/api/v1/query_range?${params}`, {
      signal: AbortSignal.timeout(LOKI_TIMEOUT_MS),
    });
    if (!r.ok) {
      return degraded(`loki HTTP ${r.status}`);
    }
    const data = (await r.json()) as {
      data?: { result?: Array<{ values?: Array<[string, string]> }> };
    };
    for (const stream of data.data?.result || []) {
      for (const [, line] of stream.values || []) {
        recordPatternLine(line, patterns, byDate);
      }
    }
  } catch (e) {
    return degraded(`loki unreachable: ${(e as Error).message || String(e)}`);
  }

  const total = Object.values(patterns).reduce((a, b) => a + b, 0);
  const byDateArr = Object.entries(byDate)
    .sort((a, b) => b[0].localeCompare(a[0]))
    .map(([date, counts]) => ({
      date,
      total: Object.values(counts ?? {}).reduce((a, b) => a + b, 0),
      counts: counts ?? {},
    }));

  const value: PatternsSummaryResponse = {
    patterns, byDate: byDateArr, total, days, source: 'loki',
  };
  cache.set(days, { at: Date.now(), value });
  return value;
}
