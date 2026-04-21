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

// eslint-disable-next-line max-lines-per-function -- #2288 pre-existing threshold violation, tracked for refactor
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

  let afterISO = `${date}T00:00:00`;
  let beforeISO = `${date}T23:59:59`;

  if (afterTime) {
    const offsetHours = deps.isEDT(date) ? 4 : 5;
    const [h, m] = afterTime.split(':').map(Number);
    const utcH = h + offsetHours;
    afterISO = `${date}T${String(utcH).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
  }
  if (beforeTime) {
    const offsetHours = deps.isEDT(date) ? 4 : 5;
    const [h, m] = beforeTime.split(':').map(Number);
    const utcH = h + offsetHours;
    if (utcH >= 24) {
      const nextDate = new Date(date);
      nextDate.setDate(nextDate.getDate() + 1);
      const nd = nextDate.toISOString().slice(0, 10);
      beforeISO = `${nd}T${String(utcH - 24).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
    } else {
      beforeISO = `${date}T${String(utcH).padStart(2, '0')}:${String(m || 0).padStart(2, '0')}:00`;
    }
  }

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
    .filter((row) => {
      const text = row.content.trim();
      if (text.startsWith('<system-reminder>')) return false;
      if (text.startsWith('<task-')) return false;
      if (text.startsWith('Base directory for this skill:')) return false;
      if (text.startsWith('[Request interrupted')) return false;
      if (text.length < 2) return false;
      return true;
    })
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
