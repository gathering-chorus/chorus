/* eslint-disable security/detect-non-literal-fs-filename --
 * Server-controlled paths joined with regex-validated session UUIDs.
 */
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

function getSessionsDir() {
  return process.env.SESSIONS_DIR || path.join(os.homedir(), 'CascadeProjects', 'jeff-bridwell-personal-site', 'data', 'sessions');
}
const MAX_SESSION_AGE_MS = 90 * 24 * 60 * 60 * 1000;

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

// #3039 — these read rrweb recordings (can be MB each) on request paths. Sync
// reads here blocked the event loop per request; listSessions read+parsed EVERY
// recording synchronously. Converted to fs.promises (off the loop). The readFile
// catch handles missing files, so the prior existsSync sync-stat is gone too.
export async function listSessions(): Promise<SessionListResponse> {
  const dir = getSessionsDir();
  const now = Date.now();
  const sessions: SessionMeta[] = [];
  let files: string[];
  try {
    files = await fs.promises.readdir(dir);
  } catch {
    return { sessions: [] };
  }
  for (const file of files) {
    if (!file.endsWith('.json')) continue;
    try {
      const filePath = path.join(dir, file);
      const stat = await fs.promises.stat(filePath);
      if (now - stat.mtimeMs > MAX_SESSION_AGE_MS) continue;
      const data = JSON.parse(await fs.promises.readFile(filePath, 'utf-8'));
      if (data.meta) sessions.push(data.meta);
    } catch { /* skip malformed */ }
  }
  sessions.sort((a, b) => new Date(b.lastActivity).getTime() - new Date(a.lastActivity).getTime());
  return { sessions };
}

export async function getSession(sessionId: string): Promise<SessionDetailResponse | null> {
  if (!isValidSessionId(sessionId)) return null;
  const filePath = path.join(getSessionsDir(), `${sessionId}.json`);
  try {
    return JSON.parse(await fs.promises.readFile(filePath, 'utf-8'));
  } catch {
    return null;
  }
}

export async function getSessionLog(sessionId: string): Promise<string | null> {
  if (!isValidSessionId(sessionId)) return null;
  const filePath = path.join(getSessionsDir(), `${sessionId}.log`);
  try {
    return await fs.promises.readFile(filePath, 'utf-8');
  } catch {
    return null;
  }
}
