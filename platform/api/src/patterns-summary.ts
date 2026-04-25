/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Server-controlled paths from env constants; indexing on validated typed keys.
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

  try {
    const r = await fetch(`${LOKI_URL}/loki/api/v1/query_range?${params}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!r.ok) {
      return { patterns, byDate: [], total: 0, days };
    }
    const data = (await r.json()) as {
      data?: { result?: Array<{ values?: Array<[string, string]> }> };
    };
    for (const stream of data.data?.result || []) {
      for (const [, line] of stream.values || []) {
        recordPatternLine(line, patterns, byDate);
      }
    }
  } catch { /* loki unreachable — return empty */ }

  const total = Object.values(patterns).reduce((a, b) => a + b, 0);
  const byDateArr = Object.entries(byDate)
    .sort((a, b) => b[0].localeCompare(a[0]))
    .map(([date, counts]) => ({
      date,
      total: Object.values(counts ?? {}).reduce((a, b) => a + b, 0),
      counts: counts ?? {},
    }));

  return { patterns, byDate: byDateArr, total, days };
}
