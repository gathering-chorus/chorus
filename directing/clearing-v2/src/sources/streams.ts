// @domain: streams
// #4597 — Jeff 16:25: "i really need a streams menu option too". Each role's recent
// commands, from the generated streams route (#4766: the spine log, read by the pulse API). v2 shows
// them as one interleaved stream (#4986); it derives nothing. An unreachable stream says so by name.
import { formatStreamTime } from './format';

export interface StreamLine { ts: string; role: string; text: string; type?: string }
/** #4986 — one stream, every role interleaved in time order, oldest first (newest at the
 *  bottom, like tail -f). Jeff 10-19 and 11-17: "interleaved across all of u most recent at bottom". */
export type Streams =
  | { ok: true; lines: StreamLine[] }
  | { ok: false; reason: string };

/** #4766 — one row of the generated streams route (pulse API, read from the spine log). */
export interface StreamEventRow { name: string; label: string; status?: string; streamRole?: string; streamTool?: string; streamPhase?: string; streamTs?: string }

/** #4766 — a call as the line Jeff reads: what it did, marked if it is running, failed or stopped. */
export function toStreamLine(r: StreamEventRow, now: Date = new Date()): StreamLine {
  const what = r.label || r.streamTool || '';
  const phase = r.streamPhase || r.status || '';
  const ts = formatStreamTime(r.streamTs, now);
  // #4991 — a reply is its own kind of line, never a call: it shows without reading as work
  if (phase === 'reply') return { ts, role: r.streamRole || '', text: `\u21a9 ${what}`, type: 'reply' };
  const text = phase === 'running' ? `\u23f3 ${what}` : phase === 'failed' ? `${what} \u2717` : phase === 'stopped' ? `${what} (stopped)` : what;
  return { ts, role: r.streamRole || '', text, type: 'call' };
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
    // rows come newest first; the tab reads oldest first, newest at the bottom
    const lines = (body.data as StreamEventRow[]).map((r) => toStreamLine(r)).filter((l) => l.role).reverse();
    return { ok: true, lines };
  } catch (e) {
    return { ok: false, reason: `stream unreachable: ${(e instanceof Error ? e.message : String(e)).split('\n')[0]}` };
  }
}
