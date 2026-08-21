/**
 * spine-tail.ts — #3607: tail-read of platform/logs/chorus.log for /api/stream.
 *
 * Extracted from server.ts, which did `fs.readFileSync` of the ENTIRE log
 * (117MB unrotated) on every 3s poll — 2.4s server time per request, event
 * loop busy ~80% while the Clearing page was open (clearing-probe fired),
 * hundreds of MB of transient allocations per poll. Same anti-pattern as the
 * 2026-06-13 535MB readFileSync lesson.
 *
 * The stream only ever renders the last ~80 lines, so we read the last
 * TAIL_BYTES of the file and parse only that. Parsing semantics are unchanged
 * — same StreamLine output, newest-first, limit*2 cap — proven by the shape
 * regression tests in tests/spine-tail.test.ts.
 */

import type fs_node from 'fs';

export type StreamLine = { ts: string; role: string; type: string; text: string; card?: string | null };

/** #3959 — the window is stated in TIME, not bytes, because bytes are a lie that
 *  drifts. 256 KB was written when it meant "thousands of lines"; at 2026-08-21
 *  volume (~50k events/day, 1.26 GB log) it measured **51 seconds**. Anything a
 *  role did a minute ago was already unreachable — which is what "no streams for
 *  20+ minutes" actually looked like from the inside.
 *
 *  8 MB holds roughly half an hour at that rate. If volume doubles the window
 *  halves, so the reader ALSO reports the span it actually covered (see
 *  readSpineLines) rather than letting a silent shrink look like a quiet team. */
export const TAIL_BYTES = 8 * 1024 * 1024;

const TURN_SKIP_PREFIXES = ['[nudge from', '[feedback]', '[response]', '[reply]', '[ack]', '[direction]', '[correction]'];
const TURN_SKIP_CONTAINS = ['<command-', 'Base directory for this skill', '[Request interrupted', '[Image:', '/var/folders'];

interface LogEntry {
  timestamp?: string;
  role?: string;
  event?: string;
  card_id?: string | number;
  phase?: string;
  state?: string;
  summary?: string;
  action?: string;
  tool_count?: string | number;
  from?: string;
  target?: string;
  tool?: string;
  elapsed_s?: string | number;
}

function formatToolDisplay(summary: string, action: string): string | null {
  if (action === 'Read' || action === 'Glob' || action === 'Grep') return null;
  if (action === 'Bash') return summary.replace(/^Bash: /, '→ ');
  if (action === 'Edit') return summary.replace(/^Edit: /, '✏️ ');
  if (action === 'Write') return summary.replace(/^Write: /, '📝 ');
  return summary;
}

function parseTurnLine(entry: LogEntry, role: string): StreamLine | null {
  let summary = (entry.summary ?? '').substring(0, 200);
  if (TURN_SKIP_PREFIXES.some((p) => summary.startsWith(p))) return null;
  if (TURN_SKIP_CONTAINS.some((p) => summary.includes(p))) return null;
  summary = summary.replace(/\s*\|\s*tools:\s*[^|]*\|\s*[\d.]+s\s*$/, '').trim();
  if (!summary) return null;
  const toolCount = parseInt(String(entry.tool_count ?? '0'), 10);
  const isJeffInput = toolCount === 0;
  if (isJeffInput && summary.length < 5) return null;
  return {
    ts: entry.timestamp ?? '',
    role: isJeffInput ? 'jeff' : role,
    type: 'turn',
    text: isJeffInput ? `→${role}: ${summary}` : summary,
  };
}

function parseToolEntry(entry: LogEntry, role: string): StreamLine | null {
  const display = formatToolDisplay((entry.summary ?? '').substring(0, 120), entry.action ?? '');
  if (display === null) return null;
  return { ts: entry.timestamp ?? '', role, type: 'tool', text: display };
}

// #2435 — canonical event is nudge.emitted. chorus-log packs the first kv as
// the JSON field; for nudge.emitted that's "from":"<sender>,to=...,content=<preview>".
function parseNudgeEntry(entry: LogEntry, role: string): StreamLine | null {
  const packed = entry.from ?? entry.target ?? '';
  const content = packed.match(/content=(.+)/)?.[1] || '';
  if (!content.includes('[gemba]')) return null;
  return { ts: entry.timestamp ?? '', role, type: 'gemba', text: content.substring(0, 200) };
}

// #3884 — werk pipeline phases render in the stream. A run executes outside
// any session, so without these Jeff's pane shows watcher sleeps during the
// most important minutes. Closed set, NOT a spine firehose: everything else
// still drops (negative-proof tested).
const WERK_PHASE_EVENTS = new Map<string, string>([
  ['commit.started', '⚙ werk: commit'],
  ['build.artifact.hashed', '⚙ werk: build'],
  ['env.up.completed', '⚙ werk: env up'],
  ['demo.test_result', '⚙ werk: tests'],
  ['demo.presented', '🎬 werk: demo presented'],
  ['merge.approved', '⚙ werk: merge approved'],
  ['deploy.completed', '🚀 werk: deploy complete'],
  ['card.branch.closed', '✅ werk: landed'],
  ['werk.failed', '🔴 werk: failed'],
]);

function parseWerkEntry(entry: LogEntry, role: string): StreamLine | null {
  // #3883's emitter shape (the one the live spine carries): event=werk.phase
  // with {phase, state}. The named-event map below stays as fallback for
  // events other emitters produce (deploy.completed from chorus-deploy, etc).
  if (entry.event === 'werk.phase') {
    const phase = entry.phase ?? '?';
    const state = entry.state ?? '';
    const mark = state === 'fail' ? '🔴' : '⚙';
    const text = state && state !== 'pass' ? `${mark} werk: ${phase} (${state})` : `${mark} werk: ${phase}`;
    const card = entry.card_id;
    return { ts: entry.timestamp ?? '', role, type: 'werk', text, card: card ? String(card) : null };
  }
  const label = WERK_PHASE_EVENTS.get(entry.event ?? '');
  if (!label) return null;
  const card = entry.card_id;
  return {
    ts: entry.timestamp ?? '',
    role,
    type: 'werk',
    text: label,
    card: card ? String(card) : null,
  };
}

/** #3959 — the running/thinking beat. #3853 built it, wired it to the spine
 *  (12,787/day), and never wired it to the pane: parseLogEntry had no branch,
 *  so every beat returned null at the fallthrough. The beat exists precisely so
 *  Jeff can see a role is alive during a long tool call; dropping it here is the
 *  reason a working role looked dead for 70 minutes. */
function parseActivityEntry(entry: LogEntry, role: string): StreamLine | null {
  const phase = entry.phase ?? '';
  if (phase !== 'running' && phase !== 'thinking') return null;
  const tool = entry.tool ? String(entry.tool) : '';
  const elapsed = entry.elapsed_s != null ? `${entry.elapsed_s}s` : '';
  const verb = phase === 'running' ? '⏳ running' : '💭 thinking';
  const text = [verb, tool, elapsed && `(${elapsed})`].filter(Boolean).join(' ');
  return {
    ts: entry.timestamp ?? '',
    role,
    type: 'activity',
    text,
    card: entry.card_id ? String(entry.card_id) : null,
  };
}

/** #3959 — why a line was dropped. Until now every rejection collapsed to
 *  `null`, so 26,000 unattributed events a day disappeared with no record and
 *  the pane looked identical to a quiet team. A drop is data. */
export type DropReason = 'no-role' | 'unknown-role' | 'event-not-rendered';

export interface SpineReadStats {
  lines: number;
  rendered: number;
  dropped: Record<DropReason, number>;
  spanFrom: string;
  spanTo: string;
}

function classifyLogEntry(entry: LogEntry): { line: StreamLine | null; drop: DropReason | null } {
  const role = entry.role ?? '';
  if (!role) return { line: null, drop: 'no-role' };
  if (!['wren', 'silas', 'kade'].includes(role)) return { line: null, drop: 'unknown-role' };
  const line = parseKnownRoleEntry(entry, role);
  return { line, drop: line ? null : 'event-not-rendered' };
}

function parseLogEntry(entry: LogEntry): StreamLine | null {
  return classifyLogEntry(entry).line;
}

function parseKnownRoleEntry(entry: LogEntry, role: string): StreamLine | null {
  const event = entry.event ?? '';
  if (event === 'session_tool') return parseToolEntry(entry, role);
  if (event === 'session_turn') return parseTurnLine(entry, role);
  if (event === 'nudge.emitted') return parseNudgeEntry(entry, role);
  if (event === 'werk.phase' || WERK_PHASE_EVENTS.has(event)) return parseWerkEntry(entry, role);
  if (event === 'agent.activity') return parseActivityEntry(entry, role);
  return null;
}

// test seam (#3884): parseLogEntry is module-private; the suite exercises it
// directly so the phase set and the no-firehose negative stay pinned.
export function parseLogEntryForTest(entry: LogEntry): StreamLine | null {
  return parseLogEntry(entry);
}

/**
 * Read the last `maxBytes` of `file` as UTF-8, dropping the partial first
 * line when the read starts mid-file. Missing/unreadable file → ''.
 */
export function tailReadUtf8(fs: typeof fs_node, file: string, maxBytes: number = TAIL_BYTES): string {
  let fd: number;
  try {
    fd = fs.openSync(file, 'r');
  } catch {
    return '';
  }
  try {
    const size = fs.fstatSync(fd).size;
    const start = Math.max(0, size - maxBytes);
    const buf = Buffer.alloc(size - start);
    fs.readSync(fd, buf, 0, buf.length, start);
    let text = buf.toString('utf-8');
    if (start > 0) {
      const nl = text.indexOf('\n');
      text = nl >= 0 ? text.slice(nl + 1) : '';
    }
    return text;
  } catch {
    return '';
  } finally {
    fs.closeSync(fd);
  }
}

/** Last `limit*2` parseable stream lines, newest first — tail-read, never the whole file. */
/** #3884 — the durable spine lives at ~/.chorus/chorus.log (never rotated;
 *  the memory layer). /api/stream once read ${CHORUS_ROOT}/platform/logs/
 *  chorus.log — a stale side-file — so werk lines never rendered live.
 *  Resolution: CHORUS_SPINE explicit override > CHORUS_HOME > HOME. */
export function spinePath(env: Record<string, string | undefined>): string {
  if (env.CHORUS_SPINE) return env.CHORUS_SPINE;
  if (env.CHORUS_HOME) return `${env.CHORUS_HOME}/chorus.log`;
  return `${env.HOME}/.chorus/chorus.log`;
}

export function readSpineLines(fs: typeof fs_node, logFile: string, limit: number): StreamLine[] {
  return readSpineWithStats(fs, logFile, limit).lines;
}

/** #3959 — the same read, but it REPORTS what it threw away and the wall-clock
 *  span it actually covered. The window is bounded in bytes; at high volume that
 *  silently shrinks, and a shrinking window looks exactly like an idle team.
 *  Never let a loss be inferred from an absence. */
export function readSpineWithStats(
  fs: typeof fs_node,
  logFile: string,
  limit: number,
): { lines: StreamLine[]; stats: SpineReadStats } {
  const out: StreamLine[] = [];
  const dropped: Record<DropReason, number> = { 'no-role': 0, 'unknown-role': 0, 'event-not-rendered': 0 };
  const logLines = tailReadUtf8(fs, logFile).trim().split('\n').filter(Boolean);
  let count = 0;
  let spanFrom = '';
  let spanTo = '';
  for (let i = logLines.length - 1; i >= 0 && count < limit * 2; i--) {
    try {
      // eslint-disable-next-line security/detect-object-injection -- i is the bounded loop index over logLines, never untrusted input (#3606)
      const entry = JSON.parse(logLines[i]) as LogEntry;
      const ts = entry.timestamp ?? '';
      if (ts) {
        if (!spanTo) spanTo = ts;
        spanFrom = ts;
      }
      const { line, drop } = classifyLogEntry(entry);
      if (line) { out.push(line); count++; }
      // eslint-disable-next-line security/detect-object-injection -- drop is a DropReason union, not untrusted input
      else if (drop) dropped[drop] += 1;
    } catch { /* ignored */ }
  }
  return {
    lines: out,
    stats: { lines: logLines.length, rendered: out.length, dropped, spanFrom, spanTo },
  };
}
