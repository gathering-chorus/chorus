/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Server-controlled paths from env constants; indexing on validated typed keys.
 */
/**
 * Jeff dashboard auxiliaries — #2099 per-page migration.
 *
 * Ports the two Gathering-only endpoints the Jeff dashboard needs:
 * - posture strip (filesystem reads from /tmp/posture-timelapse/)
 * - werk activity (Loki queries across board-client / chorus-events /
 *   chorus-session / interaction.pattern streams)
 *
 * Voice, attention, and reprompt analytics are already native on chorus-api
 * (3 of 6) — Gathering's endpoints of those names proxy here. So /borg/jeff/
 * consumes the chorus-api endpoints directly.
 */

import * as fs from 'fs';
import * as path from 'path';

const LOKI_URL = process.env.LOKI_URL || 'http://127.0.0.1:3102';
const POSTURE_BASE = process.env.POSTURE_BASE || '/tmp/posture-timelapse';

export interface PostureScore {
  posture: string;
  tension: string;
  breath: string;
  mood: string;
  energy: string;
  expression: string;
  notes: string;
  timestamp: string;
  image: string;
  date?: string;
}

export interface PostureStripResponse {
  frames: PostureScore[];
  total: number;
  filtered: number;
  days: number;
}

export function getPostureStrip(days: number, postureFilter: string, moodFilter: string): PostureStripResponse {
  const clampedDays = Math.min(Math.max(days, 1), 30);
  const frames: PostureScore[] = [];

  if (!fs.existsSync(POSTURE_BASE)) {
    return { frames: [], total: 0, filtered: 0, days: clampedDays };
  }

  try {
    const dirs = fs.readdirSync(POSTURE_BASE).sort().reverse().slice(0, clampedDays);
    for (const date of dirs) {
      const scoresFile = path.join(POSTURE_BASE, date, 'scores.jsonl');
      if (!fs.existsSync(scoresFile)) continue;
      try {
        const lines = fs.readFileSync(scoresFile, 'utf-8').trim().split('\n').filter(Boolean);
        for (const line of lines) {
          const score = JSON.parse(line) as PostureScore;
          frames.push({ ...score, date });
        }
      } catch { /* skip malformed */ }
    }
  } catch { /* skip */ }

  const filtered = frames.filter((f) => {
    if (postureFilter !== 'all' && f.posture !== postureFilter) return false;
    if (moodFilter !== 'all' && f.mood !== moodFilter) return false;
    return true;
  });

  return { frames: filtered, total: frames.length, filtered: filtered.length, days: clampedDays };
}

export interface WerkActivityEntry {
  ts: string;
  appName: string;
  event?: string;
  role?: string;
  [key: string]: unknown;
}

export interface WerkActivityResponse {
  entries: WerkActivityEntry[];
  total: number;
  hours: number;
  sources: Record<string, number>;
}

async function queryLoki(query: string, start: number, end: number, limit: string): Promise<Array<[string, string]>> {
  const params = new URLSearchParams({
    query,
    start: String(start),
    end: String(end),
    limit,
    direction: 'backward',
  });
  try {
    const r = await fetch(`${LOKI_URL}/loki/api/v1/query_range?${params}`, { signal: AbortSignal.timeout(5000) });
    if (!r.ok) return [];
    const data = (await r.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
    const out: Array<[string, string]> = [];
    for (const stream of data.data?.result || []) {
      for (const v of stream.values || []) out.push(v);
    }
    return out;
  } catch {
    return [];
  }
}

export async function getWerkActivity(hours: number, role: string, event: string): Promise<WerkActivityResponse> {
  const clamped = Math.min(Math.max(hours, 1), 168);
  const end = Date.now() * 1e6;
  const start = (Date.now() - clamped * 60 * 60 * 1000) * 1e6;

  const filters: string[] = [];
  if (role) filters.push(`role="${role.toLowerCase()}"`);
  if (event) filters.push(`event=~"${event}"`);
  const suffix = filters.length > 0 ? ' | json | ' + filters.join(' | ') : '';

  const [board, chorus, session, pattern] = await Promise.all([
    queryLoki(`{appName="board-client"}${suffix}`, start, end, '300'),
    queryLoki(`{appName="chorus-events"}${suffix}`, start, end, '300'),
    queryLoki(`{appName="chorus-session"}${suffix}`, start, end, '200'),
    queryLoki('{appName="chorus-events"} |= "interaction.pattern"', start, end, '100'),
  ]);

  const entries: WerkActivityEntry[] = [];
  const sources: Record<string, number> = { 'board-client': 0, 'chorus-events': 0, 'chorus-session': 0, 'interaction.pattern': 0 };

  function ingest(values: Array<[string, string]>, appName: string) {
    for (const [ts, line] of values) {
      try {
        const obj = JSON.parse(line);
        entries.push({ ts, appName, ...obj });
        sources[appName] = (sources[appName] || 0) + 1;
      } catch {
        entries.push({ ts, appName, raw: line });
        sources[appName] = (sources[appName] || 0) + 1;
      }
    }
  }

  ingest(board, 'board-client');
  ingest(chorus, 'chorus-events');
  ingest(session, 'chorus-session');
  ingest(pattern, 'interaction.pattern');

  entries.sort((a, b) => Number(b.ts) - Number(a.ts));

  return { entries, total: entries.length, hours: clamped, sources };
}
