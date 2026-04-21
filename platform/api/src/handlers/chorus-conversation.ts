/**
 * GET /api/chorus/conversation — cross-role conversation thread (#2188).
 *
 * Dependencies injected:
 *   db             — better-sqlite3 Database
 *   isEDT          — (dateStr) => boolean
 *   convertToLocal — (iso, tz) => string
 *
 * Query:
 *   roles  — required, comma-separated (jeff stripped since Jeff = user-author in other roles' sessions)
 *   date   — YYYY-MM-DD, default today
 *   tz     — IANA tz, default America/New_York
 *   after  — HH:MM local
 *   before — HH:MM local
 *   limit  — default 500, max 2000
 *
 * Response: {thread, participants, date, timezone, count}.
 *
 * Filters system-reminder, task-*, "Base directory", "[Request interrupted", <2-char messages.
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

export interface ChorusConversationDeps {
  db: Database.Database;
  isEDT: (dateStr: string) => boolean;
  convertToLocal: (iso: string, tz: string) => string;
}

export interface ChorusConversationQuery {
  roles?: string;
  date?: string;
  tz?: string;
  after?: string;
  before?: string;
  limit?: string;
}

interface MessageRow {
  author: string;
  content: string;
  timestamp: string;
  role: string;
  session_id: string;
}

function computeUtcISOForHour(date: string, time: string, offsetHours: number): string {
  const [h, m] = time.split(':').map(Number);
  const utcH = h + offsetHours;
  if (utcH < 24) return `${date}T${String(utcH).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
  const nextDate = new Date(date);
  nextDate.setDate(nextDate.getDate() + 1);
  const nd = nextDate.toISOString().slice(0, 10);
  return `${nd}T${String(utcH - 24).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
}

function computeWindow(date: string, afterTime: string | undefined, beforeTime: string | undefined, isEDT: (d: string) => boolean): { afterISO: string; beforeISO: string } {
  const offsetHours = isEDT(date) ? 4 : 5;
  return {
    afterISO: afterTime ? computeUtcISOForHour(date, afterTime, offsetHours) : `${date}T00:00:00`,
    beforeISO: beforeTime ? computeUtcISOForHour(date, beforeTime, offsetHours) : `${date}T23:59:59`,
  };
}

function isNoiseRow(text: string): boolean {
  if (text.startsWith('<system-reminder>')) return true;
  if (text.startsWith('<task-')) return true;
  if (text.startsWith('Base directory for this skill:')) return true;
  if (text.startsWith('[Request interrupted')) return true;
  if (text.length < 2) return true;
  return false;
}

export function fetchChorusConversation(
  deps: ChorusConversationDeps,
  query: ChorusConversationQuery,
): FetchResult {
  if (!query.roles) {
    return {
      status: 400,
      body: { error: 'Missing required parameter: roles (comma-separated, e.g. jeff,wren)' },
    };
  }

  const roles = query.roles.split(',').map((r) => r.trim().toLowerCase());
  const date = query.date || new Date().toISOString().slice(0, 10);
  const tz = query.tz || 'America/New_York';
  const afterTime = query.after;
  const beforeTime = query.before;
  const limit = Math.min(parseInt(query.limit || '500', 10), 2000);

  const roleFilter = roles.filter((r) => r !== 'jeff');
  if (roleFilter.length === 0) {
    return {
      status: 400,
      body: {
        error: 'At least one non-jeff role required (jeff is always a participant via user messages)',
      },
    };
  }

  const placeholders = roleFilter.map(() => '?').join(',');
  const { afterISO, beforeISO } = computeWindow(date, afterTime, beforeTime, deps.isEDT);

  const rows = deps.db.prepare(`
    SELECT author, content, timestamp, role, session_id
    FROM messages
    WHERE role IN (${placeholders})
    AND timestamp >= ?
    AND timestamp <= ?
    ORDER BY timestamp ASC
    LIMIT ?
  `).all(...roleFilter, afterISO, beforeISO, limit) as MessageRow[];

  const thread = rows
    .filter((row) => !isNoiseRow(row.content.trim()))
    .map((row) => ({
      speaker: row.author === 'user' ? 'jeff' : row.role,
      text: row.content.trim(),
      time: deps.convertToLocal(row.timestamp, tz),
    }));

  let filteredThread = thread;
  if (afterTime || beforeTime) {
    filteredThread = thread.filter((msg) => {
      const localHM = msg.time.split(' ')[1] || msg.time.slice(11, 16);
      if (afterTime && localHM < afterTime) return false;
      if (beforeTime && localHM >= beforeTime) return false;
      return true;
    });
  }

  return {
    status: 200,
    body: {
      thread: filteredThread,
      participants: roles,
      date,
      timezone: tz,
      count: filteredThread.length,
    },
  };
}
