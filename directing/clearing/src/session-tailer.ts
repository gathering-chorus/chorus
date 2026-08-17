/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * fs paths derived from PROJECTS_DIR (env-configurable, defaults to a server
 * path under the role's home) joined with discovered session UUIDs that match
 * /^[0-9a-f-]{36}\.jsonl$/. Object indexing on validated role-name keys.
 */
/**
 * Session JSONL Tailer — reads role session files directly.
 * Emits user input as Jeff, assistant text as role.
 * Tool calls, system reminders, metadata — all filtered.
 * Bridge = the terminal conversation minus plumbing.
 * Card: #1665
 */

import fs from 'fs';
import path from 'path';
import { MessageRouter } from './router';
import { EmitSpine, makeSpineEmitter, renderedEvent } from './reply-delivery';

// #2167: env-configurable so tests can point at a fixture directory.
const PROJECTS_DIR = process.env.CLEARING_PROJECTS_DIR || '/Users/jeffbridwell/.claude/projects';
const POLL_INTERVAL = 30000; // 30s fallback — primary delivery is fs.watch
const ROLES = ['wren', 'silas', 'kade'] as const;

// #3890/#3893 — the role's CANONICAL session dir, matched EXACTLY. The old
// substring match ('wren' ∈ entry) let ephemeral werk dirs shadow the real
// session (chorus-werk-kade-3884-…-roles-wren, EMPTY, newest wins) — which was
// the 100%-reply-loss outage Jeff called "snapchat": the tailer sat on empty
// werk transcripts while every real reply scrolled by unseen. The protected
// primitive is role-directory IS session-start (/chorus/roles/<role>/); only
// its slug is the session source. Werk transcripts are deliberately excluded.
const ROLE_DIRS: Record<string, string> = {
  wren: '-Users-jeffbridwell-CascadeProjects-chorus-roles-wren',
  silas: '-Users-jeffbridwell-CascadeProjects-chorus-roles-silas',
  kade: '-Users-jeffbridwell-CascadeProjects-chorus-roles-kade',
};

interface SessionState {
  file: string;
  offset: number;
  watcher?: fs.FSWatcher;
}

/**
 * #3887 — render a slash command as the command, not as its harness envelope.
 *
 * Claude Code wraps an invocation in `<command-message>`, `<command-name>` and
 * `<command-args>`. Jeff typed `/fuc !!!`; the room showed him the markup.
 * Third time he has named it.
 *
 * Exported and pure so the room's behaviour can be proven without a session
 * file — and so the string and array paths cannot drift apart again.
 */
export function normalizeCommandText(raw: string): string {
  const t = raw.trim();
  const name = t.match(/<command-name>([^<]+)<\/command-name>/);
  if (!name) {
    // No command envelope: ordinary typing, returned untouched. Stripping
    // angle brackets here would eat any message that legitimately quotes XML.
    return t;
  }
  const args = t.match(/<command-args>([^<]*)<\/command-args>/);
  const cmd = name[1].trim();
  const rest = args ? args[1].trim() : '';
  return rest ? `${cmd} ${rest}` : cmd;
}

export class SessionTailer {
  private router: MessageRouter;
  private sessions: Map<string, SessionState> = new Map();
  private timer: NodeJS.Timeout | null = null;
  // Debounce: buffer last assistant message per role, emit after 3s quiet (#1720)
  private pendingAssistant: Map<string, { text: string; ts: string; timer: NodeJS.Timeout }> = new Map();
  // #3772 — the return path. After a jeff-input lands in a role's session, the
  // role's REPLY must surface in the room as a visible role-response, not be
  // folded away as pm-thinking (+n steps). Mechanics: mark the role awaiting;
  // each flushed assistant text becomes the reply CANDIDATE (demoting the
  // previous candidate to pm-thinking — mid-turn status notes stay folded,
  // which is exactly what Jeff asked for: "i want the pm thinking just folded");
  // the candidate promotes to role-response after REPLY_QUIET_MS of assistant
  // silence (turn ended) or immediately when Jeff's next input arrives.
  private awaitingReply: Set<string> = new Set();
  private replyCandidate: Map<string, { text: string; ts: string; timer: NodeJS.Timeout }> = new Map();

  constructor(router: MessageRouter, emitRendered?: EmitSpine) {
    this.router = router;
    // #3864 — reply-delivery correlation: stamp reply.rendered on the spine
    // for every promoted role reply, same content hash as chorus-hooks'
    // reply.emitted. Injectable for tests; defaults to the chorus-api pulse door.
    this.emitRendered = emitRendered ?? makeSpineEmitter();
  }

  private emitRendered: EmitSpine;

  start(): void {
    // Find current session files — start from EOF, only show NEW messages
    for (const role of ROLES) {
      const sessionFile = this.findSessionFile(role);
      if (sessionFile) {
        try {
          const stats = fs.statSync(sessionFile);
          const state: SessionState = { file: sessionFile, offset: stats.size };
          // fs.watch for near-instant delivery (<100ms)
          try {
            state.watcher = fs.watch(sessionFile, () => {
              this.readNewEntries(role);
            });
          } catch { /* ignored */ }
          this.sessions.set(role, state);
        } catch { /* ignored */ }
      }
    }
    // Fallback poll for missed events and new session detection
    // #3606 — unref, same as ChorusLogTailer. These two tailers are the pair of
    // open handles that hung the nightly's clearing coverage step for 97+ minutes
    // (jest --coverage runs without --forceExit, so it waited on them forever).
    // A background fallback poll must never be the reason a process stays alive.
    this.timer = setInterval(() => this.poll(), POLL_INTERVAL);
    this.timer.unref();
  }

  getSessionCount(): number {
    return this.sessions.size;
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    for (const state of this.sessions.values()) {
      if (state.watcher) state.watcher.close();
    }
  }

  /** Read new entries for a specific role — called by fs.watch or poll */
  private readNewEntries(role: string): void {
    const state = this.sessions.get(role);
    if (!state) return;

    let stats;
    try {
      stats = fs.statSync(state.file);
    } catch { return; }

    if (stats.size <= state.offset) return;

    try {
      const fd = fs.openSync(state.file, 'r');
      const readSize = stats.size - state.offset;
      const buf = Buffer.alloc(readSize);
      const bytesRead = fs.readSync(fd, buf, 0, readSize, state.offset);
      fs.closeSync(fd);

      const data = buf.toString('utf-8', 0, bytesRead);
      const rawLines = data.split('\n');
      const lastComplete = data.endsWith('\n');
      const completeLines = lastComplete ? rawLines.filter(Boolean) : rawLines.slice(0, -1).filter(Boolean);
      const consumedBytes = lastComplete ? bytesRead : data.lastIndexOf('\n') + 1;

      state.offset += consumedBytes;

      for (const line of completeLines) {
        this.processLine(role, line);
      }
    } catch { /* ignored */ }
  }

  private findSessionFile(role: string): string | null {
    const roleDir = ROLE_DIRS[role];
    if (!roleDir) return null;

    // Find the newest .jsonl across ALL matching project dirs (#2035)
    // Multiple dirs can match (e.g. "architect" and "chorus-architect")
    try {
      const entries = fs.readdirSync(PROJECTS_DIR);
      let newest: { path: string; mtime: number } | null = null;

      for (const entry of entries) {
        if (entry !== roleDir) continue; // EXACT — substring let werk dirs shadow (see ROLE_DIRS)
        newest = this.newestJsonlIn(path.join(PROJECTS_DIR, entry), newest);
      }

      return newest ? newest.path : null;
    } catch { /* ignored */ }
    return null;
  }

  private newestJsonlIn(projDir: string, current: { path: string; mtime: number } | null): { path: string; mtime: number } | null {
    let newest = current;
    try {
      const files = fs.readdirSync(projDir)
        .filter((f) => f.endsWith('.jsonl'))
        .map((f) => {
          const fullPath = path.join(projDir, f);
          return { path: fullPath, mtime: fs.statSync(fullPath).mtimeMs };
        });
      for (const file of files) {
        if (!newest || file.mtime > newest.mtime) newest = file;
      }
    } catch { /* ignored */ }
    return newest;
  }

  /** #3913 — extracted from poll() so each half is one job: this one rebinds a
   *  role's watcher when its session file changes. Returns false when the file
   *  vanished mid-rebind (caller skips the read this tick). */
  private rebindSession(role: string): boolean {
    const state = this.sessions.get(role);
    const currentFile = this.findSessionFile(role);
    if (!currentFile || (state && state.file === currentFile)) return true;
    if (state?.watcher) state.watcher.close();
    try {
      const stats = fs.statSync(currentFile);
      const newState: SessionState = { file: currentFile, offset: stats.size };
      try {
        newState.watcher = fs.watch(currentFile, () => this.readNewEntries(role));
      } catch { /* watch unavailable — the poll fallback still reads */ }
      this.sessions.set(role, newState);
      return true;
    } catch {
      return false;
    }
  }

  private poll(): void {
    for (const role of ROLES) {
      if (!this.rebindSession(role)) continue;
      // Fallback read for anything fs.watch missed
      this.readNewEntries(role);
    }
  }

  private extractUserText(rawContent: unknown): string {
    // #3887 — a single-string content carries the SAME command envelope as the
    // array form, and this branch used to hand it to the room verbatim. That is
    // how Jeff saw `<command-message>fuc</command-message>` as a chat bubble:
    // the array path had cleaned commands up since #3852, and nobody noticed the
    // string path bypassed all of it.
    if (typeof rawContent === 'string') return normalizeCommandText(rawContent);
    if (!Array.isArray(rawContent)) return '';
    let slashCmd = '';
    const humanParts: string[] = [];
    for (const p of rawContent) {
      if (p.type !== 'text' || !p.text) continue;
      const t = p.text.trim();
      const nameMatch = t.match(/<command-name>([^<]+)<\/command-name>/);
      const argsMatch = t.match(/<command-args>([^<]*)<\/command-args>/);
      if (nameMatch) { slashCmd = nameMatch[1].trim(); continue; }
      if (argsMatch && slashCmd) { slashCmd += ' ' + argsMatch[1].trim(); continue; }
      if (t.includes('<system-reminder>') || t.includes('<command-message>')) continue;
      if (t.startsWith('Base directory for this skill') || t.startsWith('ARGUMENTS:') || t.startsWith('Stop hook')) continue;
      humanParts.push(t);
    }
    return slashCmd || humanParts.join(' ').trim();
  }

  private handleUserMessage(role: string, entry: { message?: { content?: unknown } }, ts: string): void {
    const rawContent = entry.message?.content;
    if (!rawContent) return;
    let text = this.extractUserText(rawContent);
    if (!text) return;
    text = text.replace(/\n/g, ' ');

    const nudgeMatch = text.match(/^\[nudge from (wren|silas|kade)/i);
    if (nudgeMatch) {
      this.router.ingest({ from: nudgeMatch[1].toLowerCase(), text, ts, type: 'role-response' });
    } else {
      // Jeff moved the conversation on — whatever reply was pending IS the
      // reply; promote it before his new input lands so the room reads in order.
      this.finalizeReply(role);
      this.router.ingest({ from: 'jeff', text, ts, type: 'jeff-input' });
      this.awaitingReply.add(role);
    }
  }

  // #3772 — promote the pending reply candidate to a visible role-response.
  private finalizeReply(role: string): void {
    const candidate = this.replyCandidate.get(role);
    if (!candidate) return;
    clearTimeout(candidate.timer);
    this.replyCandidate.delete(role);
    this.awaitingReply.delete(role);
    this.router.ingest({ from: role, text: candidate.text, ts: candidate.ts, type: 'role-response' });
    // #3864 — this is the moment the reply is RENDERED into Clearing; stamp
    // the join key so it pairs with the Stop hook's reply.emitted.
    this.emitRendered(renderedEvent(role, candidate.text));
  }

  private extractAssistantText(contentArr: unknown): string {
    if (typeof contentArr === 'string') return contentArr.trim();
    if (!Array.isArray(contentArr)) return '';
    const texts = contentArr
      .filter((b: { type: string }) => b.type === 'text')
      .map((b: { text: string }) => b.text || '');
    return texts.join(' ').trim();
  }

  private isFilteredAssistantText(text: string): boolean {
    if (text.match(/^DELIVERED to (wren|silas|kade)/i)) return true;
    if (text.match(/^card\.\w+/)) return true;
    if (text.match(/^(bash .*scripts\/|role-state |chorus-log )/)) return true;
    if (text.includes('[bridge]')) return true;
    if (text.includes('role.nudge.consumed')) return true;
    return false;
  }

  private handleAssistantMessage(role: string, entry: { message?: { content?: unknown; stop_reason?: string } }, ts: string): void {
    const contentArr = entry.message?.content;
    if (!contentArr) return;
    let combined = this.extractAssistantText(contentArr);
    if (!combined) return;
    combined = combined.replace(/^---\s+\w+\s+\|[^]*?---\s*/g, '').trim();
    if (!combined) return;
    if (this.isFilteredAssistantText(combined)) return;

    // #3834 — did the model STOP here, or is it mid-turn? stop_reason
    // 'end_turn' means it finished without reaching for a tool: the reply is
    // complete and there is nothing to wait for. Anything else (tool_use, or
    // absent) means more is coming, and the candidate should keep folding.
    const endedTurn = entry.message?.stop_reason === 'end_turn';

    const existing = this.pendingAssistant.get(role);
    if (existing) clearTimeout(existing.timer);
    // #3834 — the 3s debounce exists to coalesce a burst of streamed chunks.
    // Once the turn has ENDED there is no burst left to wait for, so a finished
    // reply flushes immediately. Without this, a two-word answer still cost
    // three seconds before it could even become a candidate.
    const flushDelay = endedTurn ? 0 : 3000;
    const debounceTimer = setTimeout(() => {
      const pending = this.pendingAssistant.get(role);
      if (!pending) return;
      this.pendingAssistant.delete(role);
      if (this.awaitingReply.has(role)) {
        // #3772 — this flush is the current reply candidate. The previous
        // candidate was a mid-turn status note: fold it (pm-thinking).
        const prev = this.replyCandidate.get(role);
        if (prev) {
          clearTimeout(prev.timer);
          this.router.ingest({ from: role, text: prev.text, ts: prev.ts, type: 'pm-thinking' });
        }
        const quietTimer = setTimeout(() => this.finalizeReply(role), REPLY_QUIET_MS);
        this.replyCandidate.set(role, { text: pending.text, ts: pending.ts, timer: quietTimer });
        // The turn is over — promote NOW rather than waiting out a clock that
        // has nothing left to learn.
        if (endedTurn) this.finalizeReply(role);
      } else {
        this.router.ingest({ from: role, text: pending.text, ts: pending.ts, type: 'pm-thinking' });
      }
    }, flushDelay);
    this.pendingAssistant.set(role, { text: combined, ts, timer: debounceTimer });
  }

  private processLine(role: string, line: string): void {
    let entry: { type?: string; timestamp?: string; message?: { content?: unknown; stop_reason?: string } };
    try { entry = JSON.parse(line); } catch { return; }
    const ts = entry.timestamp || new Date().toISOString();
    if (entry.type === 'user') return this.handleUserMessage(role, entry, ts);
    if (entry.type === 'assistant') return this.handleAssistantMessage(role, entry, ts);
  }
}

// #3834 — the BACKSTOP, not the mechanism.
//
// This used to be 45000, and it was the only thing deciding when a reply became
// visible. The cost, measured 2026-08-12: Jeff asked "anyone there?" at
// 16:08:04, the reply was written at 16:08:13, and he saw it three quarters of
// a minute later. His words: "it makes it impossible for any of u to give a
// simple response" and "u have to burn at least 45 seconds of tokens to say
// yes" — literally true, since the only way to make a short answer appear was
// to stay silent for 45s afterwards.
//
// A clock cannot tell "finished" from "still going". The transcript can:
// stop_reason === 'end_turn' means the assistant stopped WITHOUT a tool call,
// which is exactly a finished turn. That is now the signal (see
// handleAssistantMessage); this timer only covers the case where the turn-end
// marker never arrives — a crash, a truncated write, an older transcript
// format. Short enough to feel like a conversation, long enough to let a
// genuine multi-step turn finish.
const REPLY_QUIET_MS = Number(process.env.CLEARING_REPLY_QUIET_MS) || 8000;

/**
 * #3772 negative proof — the silent-room state must be DETECTABLE, not just
 * fixed once. Given a room transcript, find jeff-inputs that received assistant
 * activity (any pm-thinking after the input) but no visible role-response
 * before the next jeff-input. Pure function: the reply-delivery check runs it
 * over live transcripts; the test runs it over an old-style (pre-#3772)
 * transcript and MUST go red.
 */
export interface RoomRecord { from: string; text: string; ts: string; type: string }
/** What happened between one prompt and the next: did a role think, and did it
 *  ever say anything? Split out so the outer pass reads as the rule it encodes —
 *  worked but never answered — rather than as two nested loops. */
function turnAfter(records: RoomRecord[], promptIndex: number): { thoughtBy: string | null; answered: boolean } {
  let thoughtBy: string | null = null;
  for (let j = promptIndex + 1; j < records.length; j++) {
    const r = records[j];
    if (r.type === 'jeff-input') break;
    if (r.type === 'pm-thinking') thoughtBy = r.from;
    if (r.type === 'role-response') return { thoughtBy, answered: true };
  }
  return { thoughtBy, answered: false };
}

export function findSilentReplies(records: RoomRecord[]): { promptTs: string; role: string }[] {
  const violations: { promptTs: string; role: string }[] = [];
  for (let i = 0; i < records.length; i++) {
    if (records[i].type !== 'jeff-input') continue;
    const { thoughtBy, answered } = turnAfter(records, i);
    if (thoughtBy && !answered) violations.push({ promptTs: records[i].ts, role: thoughtBy });
  }
  return violations;
}
