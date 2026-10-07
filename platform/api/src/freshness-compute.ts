/**
 * #3060 (reopened 2026-10-07) — the freshness recompute, as one function that
 * runs inside the freshness worker thread (freshness-worker.ts). Its COUNT over
 * the index took 11-15s by October and froze chorus-api's main thread every 30s
 * (Silas measured it at 15:28 against the 15:10/15:16/15:23 freezes).
 */
import fs from 'node:fs';
import Database from 'better-sqlite3';
import { fetchFreshness } from './handlers/chorus-freshness';
import { SOURCE_CADENCE } from './search-meta';
import { bostonNow } from './time-utils';

export interface FreshnessResult {
  status: number;
  body: unknown;
}

export function computeFreshness(dbPath: string, spineLogPath: string): FreshnessResult {
  if (!fs.existsSync(dbPath)) {
    return { status: 503, body: { error: 'Index database not found' } };
  }
  const db = new Database(dbPath, { readonly: true });
  db.pragma('busy_timeout = 5000');
  try {
    return fetchFreshness({
      db,
      exists: (p) => fs.existsSync(p),
      spineLogPath,
      cadence: SOURCE_CADENCE,
      timestamp: bostonNow,
    });
  } finally {
    db.close();
  }
}
