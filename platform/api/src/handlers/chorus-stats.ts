/**
 * GET /api/chorus/stats — index-wide aggregate snapshot (#2188).
 *
 * Dependencies injected:
 *   db — better-sqlite3 Database handle
 *
 * Behavior:
 *   - total: row count of messages
 *   - bySource: {source → count}, ordered DESC by count
 *   - byRole: {role → count}, ordered DESC by count
 *   - dateRange: {earliest, latest} from messages.timestamp
 *   - lastIndexed: most recent watermarks.last_indexed (or null)
 *   - watermarks: all rows ordered DESC by last_indexed
 *   - refs: row count of refs
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

export interface ChorusStatsDeps {
  db: Database.Database;
}

export function fetchChorusStats(deps: ChorusStatsDeps): FetchResult {
  const { db } = deps;

  const total = (db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number }).c;

  const bySourceRows = db.prepare(
    'SELECT source, COUNT(*) as c FROM messages GROUP BY source ORDER BY c DESC',
  ).all() as Array<{ source: string; c: number }>;
  const bySource: Record<string, number> = {};
  for (const row of bySourceRows) bySource[row.source] = row.c;

  const byRoleRows = db.prepare(
    'SELECT role, COUNT(*) as c FROM messages GROUP BY role ORDER BY c DESC',
  ).all() as Array<{ role: string; c: number }>;
  const byRole: Record<string, number> = {};
  for (const row of byRoleRows) byRole[row.role] = row.c;

  const dateRange = db.prepare(
    'SELECT MIN(timestamp) as earliest, MAX(timestamp) as latest FROM messages',
  ).get() as { earliest: string | null; latest: string | null };

  const watermarks = db.prepare(
    'SELECT source, last_indexed FROM watermarks ORDER BY last_indexed DESC',
  ).all() as Array<{ source: string; last_indexed: string }>;

  const lastIndexed = watermarks.length > 0 ? watermarks[0].last_indexed : null;

  const refCount = (db.prepare('SELECT COUNT(*) as c FROM refs').get() as { c: number }).c;

  return {
    status: 200,
    body: {
      total,
      bySource,
      byRole,
      dateRange: { earliest: dateRange.earliest, latest: dateRange.latest },
      lastIndexed,
      watermarks,
      refs: refCount,
    },
  };
}
