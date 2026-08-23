/**
 * #2725 — spine fold for pending nudges: nudge.emitted minus nudge.surfaced.
 * Read projection of chorus.log (the spine). Windowed parse: last WINDOW_EVENTS
 * lines to hold <100ms p95 — the spine is never rotated, so full scans grow forever.
 *
 * Captured line shapes (2026-08-23):
 *   nudge.emitted  — payload string "from=X,to=Y,chars=N,trace=T,origin=mcp,content=..."
 *                    content is ALWAYS the last key and may contain ',' and '='.
 *   nudge.surfaced — top-level trace_id / from / to fields.
 */
import { openSync, readSync, closeSync, fstatSync } from 'fs';

export interface PendingNudge {
  trace: string;
  from: string;
  to: string;
  content: string;
  ts: string;
}

const WINDOW_EVENTS = 50_000;
const TAIL_BYTES = 16 * 1024 * 1024;

interface NudgeEvent {
  kind: 'emitted' | 'cleared'; // cleared = surfaced OR surface.failed (both exit the fold)
  trace: string;
  from: string;
  to: string;
  content: string;
  ts: string;
}

// Silas review 2026-08-23: the sync 16MB read + 50k JSON.parse must not run per
// request on chorus-api's event loop. Cache parsed nudge events keyed by
// (path,size,mtime) — the spine is append-only, so unchanged stat = unchanged tail.
let cacheKey = '';
let cacheEvents: NudgeEvent[] = [];

function parseNudgeEvents(path: string): NudgeEvent[] {
  let buf: Buffer;
  let start: number;
  let key: string;
  const fd = openSync(path, 'r');
  try {
    const st = fstatSync(fd);
    key = `${path}:${st.size}:${st.mtimeMs}`;
    if (key === cacheKey) return cacheEvents;
    start = Math.max(0, st.size - TAIL_BYTES);
    buf = Buffer.alloc(st.size - start);
    readSync(fd, buf, 0, buf.length, start);
  } finally {
    closeSync(fd);
  }
  const lines = buf.toString('utf8').split('\n');
  if (start > 0) lines.shift(); // drop partial first line
  const events: NudgeEvent[] = [];
  for (const line of lines.slice(-WINDOW_EVENTS)) {
    if (!line || !line.includes('"nudge.')) continue;
    let e: any;
    try { e = JSON.parse(line); } catch { continue; }
    if (e.event === 'nudge.emitted' && typeof e.payload === 'string') {
      const p = parsePayload(e.payload);
      if (!p.trace) continue;
      events.push({ kind: 'emitted', trace: p.trace, from: p.from ?? '', to: p.to ?? '', content: p.content ?? '', ts: String(e.timestamp ?? '') });
    } else if (e.event === 'nudge.surfaced' && typeof e.trace_id === 'string') {
      events.push({ kind: 'cleared', trace: e.trace_id, from: '', to: '', content: '', ts: '' });
    } else if (e.event === 'nudge.surface.failed' && typeof e.trace_id === 'string' && e.permanent === true) {
      // Kade review 2026-08-23: the worker emits surface.failed with permanent:false
      // on EVERY transient attempt before backoff-retrying (delivery-worker.ts:299)
      // — clearing on those loses a nudge whose retries are still running. Only a
      // permanent failure exits the fold: pending = emitted − surfaced − failed(permanent).
      // Divergence note: the Rust folds (nudge_poll.rs, pulse.rs assemble_nudges)
      // ignore surface.failed entirely — their pending can disagree with this one
      // after a permanent failure until fold-convergence lands.
      events.push({ kind: 'cleared', trace: e.trace_id, from: '', to: '', content: '', ts: '' });
    }
  }
  cacheKey = key;
  cacheEvents = events;
  return events;
}

/** Parse "from=X,to=Y,...,content=..." — content is last and may contain , and = */
function parsePayload(payload: string): Record<string, string> {
  const out: Record<string, string> = {};
  const contentIdx = payload.indexOf('content=');
  const head = contentIdx >= 0 ? payload.slice(0, contentIdx) : payload;
  if (contentIdx >= 0) out.content = payload.slice(contentIdx + 'content='.length);
  for (const part of head.split(',')) {
    const eq = part.indexOf('=');
    if (eq > 0) out[part.slice(0, eq).trim()] = part.slice(eq + 1);
  }
  return out;
}

/** #2725 AC5 — recent nudge.emitted events from the spine tail, cleared or not:
 *  the card-story consumer wants history, not just pending. */
export function recentNudges(logPath: string, limit = 100): PendingNudge[] {
  const out: PendingNudge[] = [];
  for (const e of parseNudgeEvents(logPath)) {
    if (e.kind === 'emitted') out.push({ trace: e.trace, from: e.from, to: e.to, content: e.content, ts: e.ts });
  }
  return out.slice(-limit);
}

export function buildNudgeFold(
  logPath: string,
  role: string,
  opts: { all?: boolean } = {},
): PendingNudge[] {
  const emitted = new Map<string, PendingNudge>();
  const cleared = new Set<string>();

  for (const e of parseNudgeEvents(logPath)) {
    if (e.kind === 'emitted') {
      if (opts.all || e.to === role) {
        emitted.set(e.trace, { trace: e.trace, from: e.from, to: e.to, content: e.content, ts: e.ts });
      }
    } else {
      cleared.add(e.trace);
    }
  }

  // a cleared trace exits regardless of line order within the window (AC8)
  return [...emitted.values()].filter((n) => !cleared.has(n.trace));
}
