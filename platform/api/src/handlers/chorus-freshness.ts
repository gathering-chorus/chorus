/**
 * GET /api/chorus/freshness — Per-source freshness with graduated staleness (extracted #2189).
 *
 * Queries the chorus index for per-source watermarks, aggregates by source
 * prefix (claude, spine, artifact:adr, …), and assigns a staleness level:
 *   - For "countable" sources (claude, spine) with known on-disk counts:
 *       drift = onDisk - indexed;
 *         0           → fresh
 *         1..99       → warn
 *         100..999    → critical
 *         1000+       → dead
 *   - For the rest: ratio = age_seconds / expected_cadence_seconds;
 *         ≤1.5 → fresh, ≤3 → warn, ≤7 → critical, else dead.
 *
 * Dependencies injected:
 *   db            — better-sqlite3 Database
 *   exists        — filesystem existence check (for spine log)
 *   readFile      — filesystem read (for spine log line count)
 *   now           — epoch ms
 *   timestamp     — wall-clock string for response
 *   spineLogPath  — path to chorus.log
 *   cadence       — source → cadence_seconds map
 */
import type Database from 'better-sqlite3';

export interface FreshnessDeps {
  db: Database.Database;
  exists?: (p: string) => boolean;
  readFile?: (p: string, enc: BufferEncoding) => string;
  now?: () => number;
  timestamp?: () => string;
  spineLogPath: string;
  cadence?: Record<string, number>;
}

const DEFAULT_CADENCE: Record<string, number> = {
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

export interface FreshnessSource {
  source: string;
  last_indexed: string;
  age_seconds: number;
  expected_cadence: number;
  staleness_ratio: number;
  unindexed: number;
  level: 'fresh' | 'warn' | 'critical' | 'dead';
}

export interface FreshnessBody {
  sources: FreshnessSource[];
  summary: { total_sources: number; fresh: number; warn: number; critical: number; dead: number };
  timestamp: string;
}

export interface FreshnessResult {
  status: number;
  body: FreshnessBody | { error: string };
}

interface Watermark { source: string; last_indexed: string }
interface Count { cnt: number }

export function fetchFreshness({
  db,
  exists = () => false,
  readFile = () => '',
  now = Date.now,
  timestamp = () => new Date().toISOString(),
  spineLogPath,
  cadence = DEFAULT_CADENCE,
}: FreshnessDeps): FreshnessResult {
  const watermarks = db.prepare('SELECT source, last_indexed FROM watermarks ORDER BY source').all() as Watermark[];

  const nowMs = now();
  const aggregated = new Map<string, string>();
  for (const w of watermarks) {
    const parts = w.source.split(':');
    const key = parts[0] === 'artifact' ? parts.slice(0, 2).join(':') : parts[0];
    const existing = aggregated.get(key);
    if (!existing || w.last_indexed > existing) {
      aggregated.set(key, w.last_indexed);
    }
  }

  const claudeIndexed = (db.prepare("SELECT COUNT(*) as cnt FROM messages WHERE source='claude'").get() as Count).cnt;
  const claudeWatermarks = (db.prepare("SELECT COUNT(*) as cnt FROM watermarks WHERE source LIKE 'claude:%'").get() as Count).cnt;
  const claudeOnDisk = claudeWatermarks;

  const spineOnDisk = exists(spineLogPath) ? readFile(spineLogPath, 'utf-8').split('\n').length : 0;
  const spineIndexed = (db.prepare("SELECT COUNT(*) as cnt FROM messages WHERE source='spine'").get() as Count).cnt;

  const driftMap: Partial<Record<string, { onDisk: number; indexed: number }>> = {
    claude: { onDisk: claudeOnDisk, indexed: claudeIndexed },
    spine: { onDisk: spineOnDisk, indexed: spineIndexed },
  };

  const sources: FreshnessSource[] = Array.from(aggregated.entries()).map(([source, lastIndexed]) => {
    const lastMs = new Date(lastIndexed).getTime();
    const ageSecs = Math.floor((nowMs - lastMs) / 1000);
    const cadenceKey = source.split(':')[0];
    const cadenceVal = cadence[cadenceKey] || cadence[source] || 86400;
    const ratio = ageSecs / cadenceVal;

    const drift = driftMap[cadenceKey];
    let level: FreshnessSource['level'];
    let unindexed = 0;
    if (drift) {
      unindexed = Math.max(0, drift.onDisk - drift.indexed);
      if (unindexed === 0) level = 'fresh';
      else if (unindexed < 100) level = 'warn';
      else if (unindexed < 1000) level = 'critical';
      else level = 'dead';
    } else {
      if (ratio <= 1.5) level = 'fresh';
      else if (ratio <= 3) level = 'warn';
      else if (ratio <= 7) level = 'critical';
      else level = 'dead';
    }

    return {
      source,
      last_indexed: lastIndexed,
      age_seconds: ageSecs,
      expected_cadence: cadenceVal,
      staleness_ratio: Math.round(ratio * 10) / 10,
      unindexed,
      level,
    };
  });

  const summary = {
    total_sources: sources.length,
    fresh: sources.filter((s) => s.level === 'fresh').length,
    warn: sources.filter((s) => s.level === 'warn').length,
    critical: sources.filter((s) => s.level === 'critical').length,
    dead: sources.filter((s) => s.level === 'dead').length,
  };

  return { status: 200, body: { sources, summary, timestamp: timestamp() } };
}
