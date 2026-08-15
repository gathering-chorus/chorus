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

/** Last ~256KB of the log — thousands of lines; the page asks for ≤ ~80. */
export const TAIL_BYTES = 256 * 1024;

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

function parseLogEntry(entry: LogEntry): StreamLine | null {
  const role = entry.role ?? '';
  if (!role || !['wren', 'silas', 'kade'].includes(role)) return null;
  const event = entry.event ?? '';
  if (event === 'session_tool') return parseToolEntry(entry, role);
  if (event === 'session_turn') return parseTurnLine(entry, role);
  if (event === 'nudge.emitted') return parseNudgeEntry(entry, role);
  if (event === 'werk.phase' || WERK_PHASE_EVENTS.has(event)) return parseWerkEntry(entry, role);
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
export function readSpineLines(fs: typeof fs_node, logFile: string, limit: number): StreamLine[] {
  const out: StreamLine[] = [];
  const logLines = tailReadUtf8(fs, logFile).trim().split('\n').filter(Boolean);
  let count = 0;
  for (let i = logLines.length - 1; i >= 0 && count < limit * 2; i--) {
    try {
      // eslint-disable-next-line security/detect-object-injection -- i is the bounded loop index over logLines, never untrusted input (#3606)
      const line = parseLogEntry(JSON.parse(logLines[i]));
      if (line) { out.push(line); count++; }
    } catch { /* ignored */ }
  }
  return out;
}
