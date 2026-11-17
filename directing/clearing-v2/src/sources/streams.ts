// @domain: streams
// #4597 — Jeff 16:25: "i really need a streams menu option too". Each role's recent
// commands, from the generated streams route (#4766: the spine log, read by the pulse API). v2 groups
// them by role and shows them; it derives nothing. An unreachable stream says so by name.
import { formatBostonTime } from './format';

export interface StreamLine { ts: string; role: string; text: string; type?: string }
export type Streams =
  | { ok: true; roles: Array<{ role: string; lines: StreamLine[] }> }
  | { ok: false; reason: string };

/** Lines per role on the Streams tab. */
export const STREAM_LINES_PER_ROLE = 15;

export function groupByRole(lines: StreamLine[], perRole = STREAM_LINES_PER_ROLE): Array<{ role: string; lines: StreamLine[] }> {
  const byRole = new Map<string, StreamLine[]>();
  for (const l of lines) {
    if (!l || !l.role) continue;
    if (!byRole.has(l.role)) byRole.set(l.role, []);
    byRole.get(l.role)!.push(l);
  }
  // newest first within each role; roles in name order so the columns never jump
  return Array.from(byRole.keys()).sort().map((role) => ({ role, lines: byRole.get(role)!.slice(-perRole).reverse() }));
}

/** #4766 — one row of the generated streams route (pulse API, read from the spine log). */
export interface StreamEventRow { name: string; label: string; status?: string; streamRole?: string; streamTool?: string; streamPhase?: string; streamTs?: string }

/** #4766 — a call as the line Jeff reads: what it did, marked if it is running, failed or stopped. */
export function toStreamLine(r: StreamEventRow): StreamLine {
  const what = r.label || r.streamTool || '';
  const phase = r.streamPhase || r.status || '';
  const text = phase === 'running' ? `\u23f3 ${what}` : phase === 'failed' ? `${what} \u2717` : phase === 'stopped' ? `${what} (stopped)` : what;
  return { ts: formatBostonTime(r.streamTs), role: r.streamRole || '', text, type: 'call' };
}

/** The generated streams route: GET <url>?limit=200 answers { data: [rows newest first] }.
 *  #4766 — never the old Clearing (:3470); an unreadable route says so by name. */
export async function fetchStreams(streamUrl: string): Promise<Streams> {
  if (!streamUrl) return { ok: false, reason: 'stream not configured' };
  try {
    const res = await fetch(`${streamUrl}?limit=200`, { signal: AbortSignal.timeout(5000) });
    if (!res.ok) return { ok: false, reason: `stream answered ${res.status}` };
    const body = (await res.json()) as { data?: unknown } | null;
    if (!body || !Array.isArray(body.data)) return { ok: false, reason: 'stream returned no list' };
    // rows come newest first; groupByRole takes them oldest first
    const lines = (body.data as StreamEventRow[]).map(toStreamLine).reverse();
    return { ok: true, roles: groupByRole(lines) };
  } catch (e) {
    return { ok: false, reason: `stream unreachable: ${(e instanceof Error ? e.message : String(e)).split('\n')[0]}` };
  }
}
