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
  // #3982 — was: drop Read/Glob/Grep. Jeff, 2026-08-22: "i want all of ur
  // calls not a subset." A role reading for two minutes IS working; hiding
  // that is how a busy role reads as dead.
  if (action === 'Read') return summary.replace(/^Read: /, '\u25cb ');
  if (action === 'Glob' || action === 'Grep') return summary.replace(/^(Glob|Grep): /, '\u25cb ');
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

/** #3982 — the per-call action, which is what a BUSY role actually emits.
 *
 *  agent.activity is a heartbeat: it fires every 10s and ONLY when a session
 *  has been idle >= 15s (main.rs:181). A role working in bursts under 15
 *  seconds therefore emits agent.action + hook.decision and nothing the pane
 *  rendered — so it looked silent exactly when it was busiest, while a role
 *  stuck on one slow call looked alive. The pane showed the inverse of the truth.
 *
 *  Jeff, 2026-08-22, while I was mid-tool-call: "i see no streams for minutes
 *  from u." */
function parseActionEntry(entry: LogEntry, role: string): StreamLine | null {
  const tool = entry.tool ? String(entry.tool) : '';
  if (!tool) return null;
  // EVERY call, no quiet class. Jeff, 2026-08-22: "i want all of ur calls not
  // a subset." Read/Glob/Grep were dropped here as noise, but a role reading
  // for two minutes IS working, and hiding it is how a busy role reads as dead.
  // The pane's job is to show what happened, not to curate it.
  return {
    ts: entry.timestamp ?? '',
    role,
    type: 'action',
    text: `\u25b8 ${tool}`,
    card: entry.card_id ? String(entry.card_id) : null,
  };
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
  if (event === 'agent.action') return parseActionEntry(entry, role);
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
export function spinePath(
  env: Record<string, string | undefined>,
  exists: (p: string) => boolean = (p) => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    try { (require('fs') as typeof fs_node).statSync(p); return true; } catch { return false; }
  },
): string {
  if (env.CHORUS_SPINE) return env.CHORUS_SPINE;
  // #2725 (2026-08-24) — CHORUS_HOME is ambiguous by convention: the repo in a
  // shell, ~/.chorus to a service. Taken on faith it points at a path that may
  // not exist, and the pane then renders a DEAD log — the same silent-stale
  // shape that ate Jeff's nudges (an 84KB leftover, months old, no error).
  // Resolve to the first candidate that EXISTS: a path that isn't there cannot
  // be the never-rotated spine.
  const candidates = [
    env.CHORUS_HOME ? `${env.CHORUS_HOME}/chorus.log` : undefined,
    env.HOME ? `${env.HOME}/.chorus/chorus.log` : undefined,
  ].filter((p): p is string => !!p);
  for (const p of candidates) if (exists(p)) return p;
  return candidates[candidates.length - 1] ?? `${env.HOME}/.chorus/chorus.log`;
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

/** #3982 — THE single projection. One read of the spine produces BOTH products:
 *  the rendered stream lines AND per-role activity for the tiles.
 *
 *  They were two independent reads with different windows and different accept
 *  rules — tiles took 400 KB and ANY event name, the pane took 8 MB and its
 *  named set. Same file, two answers, so a tile could say "building 45s ago"
 *  off an event the pane is built to ignore. On 2026-08-22 06:19 the two
 *  surfaces were 36 minutes apart about the same role.
 *
 *  Jeff: "if u check the role state and streams they MUST match at any given
 *  time." Not a tolerance — a shared source. With one pass there is nothing
 *  left to disagree about.
 */
export interface SpineProjection {
  lines: StreamLine[];
  /** role -> { ageSecs, kind } for the newest role-attributed event. */
  activity: Record<string, { ageSecs: number; kind: string }>;
  stats: SpineReadStats;
}

export function projectSpine(
  fs: typeof fs_node,
  logFile: string,
  limit: number,
  now: number,
  agentRoles: ReadonlySet<string>,
): SpineProjection {
  const { lines, stats } = readSpineWithStats(fs, logFile, limit);

  // #2725 (2026-08-24) — ONE accept rule, not two.
  //
  // This loop used to walk the RAW spine while the pane rendered `lines`, so
  // the tile and the pane could still disagree about the same role from the
  // same file: the tile counted `system.heartbeat` (a timer the process emits
  // whether or not the role does anything) and called silas active "1s ago"
  // while his pane sat 7 minutes silent — the #3976 reconciliation flow caught
  // exactly that, live, three roles at once. #3982 removed the second READ;
  // this removes the second RULE.
  //
  // Activity is now the newest line the pane actually renders. Tile and pane
  // cannot drift, because there is nothing left to drift between: what Jeff
  // sees in the stream IS what the tile is claiming.
  const act = new Map<string, { ageSecs: number; kind: string }>();
  for (const l of lines) {
    if (!agentRoles.has(l.role) || !l.ts) continue;
    const t = Date.parse(l.ts);
    if (Number.isNaN(t)) continue;
    const ageSecs = Math.max(0, Math.round((now - t) / 1000));
    const prev = act.get(l.role);
    if (!prev || ageSecs < prev.ageSecs) act.set(l.role, { ageSecs, kind: l.type });
  }
  return { lines, activity: Object.fromEntries(act), stats };
}
