/**
 * GET /api/chorus/reconcile — post-session catch-up snapshot (#2188).
 *
 * What did the other roles (and Slack, and Jeff) do while I was away?
 *
 * Dependencies injected:
 *   db  — better-sqlite3 Database
 *   now — () => number, epoch ms (default Date.now), used for 24h fallback cutoff
 *
 * Query params:
 *   role — required, must be one of {wren, silas, kade}
 *
 * Cutoff:
 *   - Last timestamp of channel='session:<role>' + author='assistant' in messages
 *   - Fallback: now() - 24h if no prior session row exists
 *
 * Response shape:
 *   - slack: {channel → Array<{channel, role, author, content, timestamp}>}, ≤5 per channel
 *   - sessions: {roleName → message count}, other roles since cutoff
 *   - jeffDirection: Array<{channel, content, timestamp}>, user-authored messages, 10 most recent
 *   - stats: { total: int, bySource: {source → count} }
 *
 * Slack filter: drop is_bridge=1 rows where the bridged role matches the caller.
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

const VALID_ROLES = new Set(['wren', 'silas', 'kade']);
const TWENTY_FOUR_HOURS_MS = 24 * 60 * 60 * 1000;

export interface ChorusReconcileDeps {
  db: Database.Database;
  now?: () => number;
}

export interface ChorusReconcileQuery {
  role?: string;
}

interface SlackMsg {
  channel: string;
  role: string;
  author: string;
  content: string;
  timestamp: string;
}

interface SessionRow {
  channel: string;
  count: number;
}

interface JeffMsg {
  channel: string;
  content: string;
  timestamp: string;
}

export function fetchChorusReconcile(
  deps: ChorusReconcileDeps,
  query: ChorusReconcileQuery,
): FetchResult {
  const role = query.role ?? '';
  if (!role || !VALID_ROLES.has(role)) {
    return {
      status: 400,
      body: { error: 'Missing or invalid parameter: role (wren|silas|kade)' },
    };
  }

  const now = deps.now ?? Date.now;
  const { db } = deps;

  const lastSession = db.prepare(
    `SELECT MAX(timestamp) as ts FROM messages
     WHERE source = 'claude' AND channel = ? AND author = 'assistant'`,
  ).get(`session:${role}`) as { ts: string | null };

  const cutoff = lastSession?.ts ?? new Date(now() - TWENTY_FOUR_HOURS_MS).toISOString();

  const slackMessages = db.prepare(
    `SELECT channel, role, author, content, timestamp FROM messages
     WHERE source = 'slack' AND timestamp > ?
       AND NOT (is_bridge = 1 AND role = ?)
     ORDER BY timestamp ASC`,
  ).all(cutoff, role) as SlackMsg[];

  const slackByChannel: Record<string, SlackMsg[]> = {};
  for (const msg of slackMessages) {
    if (!slackByChannel[msg.channel]) slackByChannel[msg.channel] = [];
    if (slackByChannel[msg.channel].length < 5) {
      slackByChannel[msg.channel].push(msg);
    }
  }

  const sessionRows = db.prepare(
    `SELECT channel, COUNT(*) as count FROM messages
     WHERE source = 'claude' AND channel != ? AND timestamp > ?
     GROUP BY channel`,
  ).all(`session:${role}`, cutoff) as SessionRow[];

  const sessions: Record<string, number> = {};
  for (const row of sessionRows) {
    sessions[row.channel.replace('session:', '')] = row.count;
  }

  const jeffDirection = db.prepare(
    `SELECT channel, content, timestamp FROM messages
     WHERE source = 'claude' AND author = 'user' AND timestamp > ?
     ORDER BY timestamp DESC
     LIMIT 10`,
  ).all(cutoff) as JeffMsg[];

  const total = (db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number }).c;
  const bySourceRows = db.prepare(
    'SELECT source, COUNT(*) as c FROM messages GROUP BY source',
  ).all() as Array<{ source: string; c: number }>;
  const bySource: Record<string, number> = {};
  for (const row of bySourceRows) bySource[row.source] = row.c;

  return {
    status: 200,
    body: {
      slack: slackByChannel,
      sessions,
      jeffDirection,
      stats: { total, bySource },
    },
  };
}
