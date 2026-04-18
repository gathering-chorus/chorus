/**
 * GET /api/chorus/rcas — list RCAs with optional status filter (#2188).
 *
 * Dependencies injected:
 *   db — better-sqlite3 Database (rcas table)
 *
 * Query:
 *   status — optional; one of {open, verified, closed}; silently ignored if invalid
 *
 * Behavior:
 *   - Rows ordered by created_at DESC
 *   - JSON string columns (contributing_factors, corrective_actions, cards, spine_events) parsed
 *   - trigger_event renamed to trigger in response
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

const VALID_STATUSES = new Set(['open', 'verified', 'closed']);

export interface ChorusRcasDeps {
  db: Database.Database;
}

export interface ChorusRcasQuery {
  status?: string;
}

interface RcaRow {
  id: number;
  title: string;
  trigger_event: string;
  timeline: string;
  root_cause: string;
  contributing_factors: string;
  corrective_actions: string;
  cards: string;
  spine_events: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export function fetchChorusRcas(deps: ChorusRcasDeps, query: ChorusRcasQuery): FetchResult {
  let sql = 'SELECT * FROM rcas';
  const params: string[] = [];
  if (query.status && VALID_STATUSES.has(query.status)) {
    sql += ' WHERE status = ?';
    params.push(query.status);
  }
  sql += ' ORDER BY created_at DESC';

  const rows = deps.db.prepare(sql).all(...params) as RcaRow[];

  const results = rows.map((row) => ({
    id: row.id,
    title: row.title,
    trigger: row.trigger_event,
    timeline: row.timeline,
    root_cause: row.root_cause,
    contributing_factors: JSON.parse(row.contributing_factors),
    corrective_actions: JSON.parse(row.corrective_actions),
    cards: JSON.parse(row.cards),
    spine_events: JSON.parse(row.spine_events),
    status: row.status,
    created_at: row.created_at,
    updated_at: row.updated_at,
  }));

  return { status: 200, body: { results, total: results.length } };
}
