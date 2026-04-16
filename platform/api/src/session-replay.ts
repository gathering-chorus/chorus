/**
 * Session Replay — #2099 per-page migration from Gathering.
 *
 * Reads rrweb session recordings from Gathering's data/sessions/ directory
 * (that's where the browser recorder writes them). chorus-api exposes the
 * list + get endpoints so /borg/replay/ can render the sessions via rrweb
 * player without going through Gathering's admin-auth middleware.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const SESSIONS_DIR = process.env.SESSIONS_DIR || path.join(os.homedir(), 'CascadeProjects', 'jeff-bridwell-personal-site', 'data', 'sessions');
const MAX_SESSION_AGE_MS = 7 * 24 * 60 * 60 * 1000;

interface SessionMeta {
  sessionId: string;
  startTime: string;
  lastActivity: string;
  pages: string[];
  eventCount: number;
}

export interface SessionListResponse {
  sessions: SessionMeta[];
}

export interface SessionDetailResponse {
  meta: SessionMeta;
  events: unknown[];
}

export function isValidSessionId(id: string): boolean {
  return /^ses_\d+_[a-z0-9]+$/.test(id);
}

export function listSessions(): SessionListResponse {
  if (!fs.existsSync(SESSIONS_DIR)) return { sessions: [] };
  const now = Date.now();
  const sessions: SessionMeta[] = [];
  try {
    for (const file of fs.readdirSync(SESSIONS_DIR)) {
      if (!file.endsWith('.json')) continue;
      try {
        const filePath = path.join(SESSIONS_DIR, file);
        const stat = fs.statSync(filePath);
        if (now - stat.mtimeMs > MAX_SESSION_AGE_MS) continue;
        const data = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
        if (data.meta) sessions.push(data.meta);
      } catch { /* skip malformed */ }
    }
  } catch { /* skip */ }
  sessions.sort((a, b) => new Date(b.lastActivity).getTime() - new Date(a.lastActivity).getTime());
  return { sessions };
}

export function getSession(sessionId: string): SessionDetailResponse | null {
  if (!isValidSessionId(sessionId)) return null;
  const filePath = path.join(SESSIONS_DIR, `${sessionId}.json`);
  if (!fs.existsSync(filePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  } catch {
    return null;
  }
}

export function getSessionLog(sessionId: string): string | null {
  if (!isValidSessionId(sessionId)) return null;
  const filePath = path.join(SESSIONS_DIR, `${sessionId}.log`);
  if (!fs.existsSync(filePath)) return null;
  try {
    return fs.readFileSync(filePath, 'utf-8');
  } catch {
    return null;
  }
}
