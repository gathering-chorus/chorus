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

function tailLines(path: string): string[] {
  const fd = openSync(path, 'r');
  try {
    const size = fstatSync(fd).size;
    const start = Math.max(0, size - TAIL_BYTES);
    const buf = Buffer.alloc(size - start);
    readSync(fd, buf, 0, buf.length, start);
    const lines = buf.toString('utf8').split('\n');
    if (start > 0) lines.shift(); // drop partial first line
    return lines.slice(-WINDOW_EVENTS);
  } finally {
    closeSync(fd);
  }
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

/** #2725 AC5 (was AC9) — recent nudge.emitted events from the spine tail,
 *  surfaced or not: the card-story consumer wants history, not just pending. */
export function recentNudges(logPath: string, limit = 100): PendingNudge[] {
  const out: PendingNudge[] = [];
  for (const line of tailLines(logPath)) {
    if (!line) continue;
    let e: any;
    try { e = JSON.parse(line); } catch { continue; }
    if (e.event !== 'nudge.emitted' || typeof e.payload !== 'string') continue;
    const p = parsePayload(e.payload);
    if (!p.trace) continue;
    out.push({
      trace: p.trace,
      from: p.from ?? '',
      to: p.to ?? '',
      content: p.content ?? '',
      ts: String(e.timestamp ?? ''),
    });
  }
  return out.slice(-limit);
}

export function buildNudgeFold(
  logPath: string,
  role: string,
  opts: { all?: boolean } = {},
): PendingNudge[] {
  const emitted = new Map<string, PendingNudge>();
  const surfacedTraces = new Set<string>();

  for (const line of tailLines(logPath)) {
    if (!line) continue;
    let e: any;
    try {
      e = JSON.parse(line);
    } catch {
      continue; // non-JSON spine lines are not nudge events
    }
    if (e.event === 'nudge.emitted' && typeof e.payload === 'string') {
      const p = parsePayload(e.payload);
      if (!p.trace) continue;
      if (opts.all || p.to === role) {
        emitted.set(p.trace, {
          trace: p.trace,
          from: p.from ?? '',
          to: p.to ?? '',
          content: p.content ?? '',
          ts: String(e.timestamp ?? ''),
        });
      }
    } else if (e.event === 'nudge.surfaced' && typeof e.trace_id === 'string') {
      surfacedTraces.add(e.trace_id);
    }
  }

  // surfaced clears the trace regardless of line order within the window (AC8)
  return [...emitted.values()].filter((n) => !surfacedTraces.has(n.trace));
}
