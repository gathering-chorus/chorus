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

const PROJECTS_DIR = '/Users/jeffbridwell/.claude/projects';
const POLL_INTERVAL = 30000; // 30s fallback — primary delivery is fs.watch
const ROLES = ['wren', 'silas', 'kade'] as const;

// Map role to its project directory pattern
const ROLE_DIRS: Record<string, string> = {
  wren: 'product-manager',
  silas: 'architect',
  kade: 'engineer',
};

interface SessionState {
  file: string;
  offset: number;
  watcher?: fs.FSWatcher;
}

export class SessionTailer {
  private router: MessageRouter;
  private sessions: Map<string, SessionState> = new Map();
  private timer: NodeJS.Timeout | null = null;
  // Debounce: buffer last assistant message per role, emit after 3s quiet (#1720)
  private pendingAssistant: Map<string, { text: string; ts: string; timer: NodeJS.Timeout }> = new Map();

  constructor(router: MessageRouter) {
    this.router = router;
  }

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
          } catch {}
          this.sessions.set(role, state);
        } catch {}
      }
    }
    // Fallback poll for missed events and new session detection
    this.timer = setInterval(() => this.poll(), POLL_INTERVAL);
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
    } catch {}
  }

  private findSessionFile(role: string): string | null {
    const roleDir = ROLE_DIRS[role];
    if (!roleDir) return null;

    // Find the project dir matching this role
    try {
      const entries = fs.readdirSync(PROJECTS_DIR);
      for (const entry of entries) {
        if (entry.includes(roleDir)) {
          const projDir = path.join(PROJECTS_DIR, entry);
          // Find newest .jsonl file
          const files = fs.readdirSync(projDir)
            .filter(f => f.endsWith('.jsonl'))
            .map(f => ({
              name: f,
              path: path.join(projDir, f),
              mtime: fs.statSync(path.join(projDir, f)).mtimeMs,
            }))
            .sort((a, b) => b.mtime - a.mtime);
          if (files.length > 0) return files[0].path;
        }
      }
    } catch {}
    return null;
  }

  private poll(): void {
    for (const role of ROLES) {
      const state = this.sessions.get(role);

      // Check if session file changed (new session started)
      const currentFile = this.findSessionFile(role);
      if (currentFile && (!state || state.file !== currentFile)) {
        // Close old watcher
        if (state?.watcher) state.watcher.close();
        try {
          const stats = fs.statSync(currentFile);
          const newState: SessionState = { file: currentFile, offset: stats.size };
          try {
            newState.watcher = fs.watch(currentFile, () => this.readNewEntries(role));
          } catch {}
          this.sessions.set(role, newState);
        } catch { continue; }
      }

      // Fallback read for anything fs.watch missed
      this.readNewEntries(role);
    }
  }

  private processLine(role: string, line: string): void {
    let entry;
    try {
      entry = JSON.parse(line);
    } catch {
      return;
    }

    const msgType = entry.type;
    const ts = entry.timestamp || new Date().toISOString();

    if (msgType === 'user') {
      // Input from role terminals — classify as Jeff or role-to-role (#1706)
      const rawContent = entry.message?.content;
      if (!rawContent) return;

      let text = '';
      if (typeof rawContent === 'string') {
        text = rawContent.trim();
      } else if (Array.isArray(rawContent)) {
        const textParts = rawContent
          .filter((p: { type: string; text?: string }) =>
            p.type === 'text' && p.text &&
            !p.text.includes('<command-') &&
            !p.text.includes('<system-reminder>') &&
            !p.text.includes('Base directory for this skill') &&
            !p.text.startsWith('ARGUMENTS:'))
          .map((p: { text: string }) => p.text.trim());
        text = textParts.join(' ').trim();
      }

      if (!text) return;
      text = text.replace(/\n/g, ' ');

      // Filter system noise — not Jeff's words
      if (/<[a-z-]+>/i.test(text)) return;
      if (text.includes('/Users/') || text.includes('/var/') || text.includes('/private/') || text.includes('/tmp/')) return;
      if (text.includes('[Image: source:')) return;
      if (text.includes('[Request interrupted')) return;
      if (text.startsWith('ARGUMENTS:') || text.startsWith('Base directory') || text.startsWith('Stop hook')) return;
      // Filter role-to-role nudges injected into terminal
      if (text.includes('[nudge from ')) return;
      if (text.includes('role.nudge.consumed')) return;
      if (text.match(/^\[(reply|ack|feedback|direction|correction)\]/i)) return;
      // Filter hook echo — bridge events reflected back through session
      if (text.includes('[bridge]')) return;
      // Filter acceptance commit messages echoed back
      if (text.match(/^Accepted #\d+\s*—/)) return;
      // Filter slash commands (skill expansions)
      if (text.startsWith('/') && text.length < 20) return;

      this.router.ingest({
        from: 'jeff',
        text,
        ts,
        type: 'jeff-input',
      });
    } else if (msgType === 'assistant') {
      // Surface ALL roles' reasoning/thinking on Bridge (#1706, Jeff comment 3)
      // Terminal output is ephemeral — Bridge is the persistent record.
      const contentArr = entry.message?.content;
      if (!contentArr) return;

      let texts: string[] = [];
      if (typeof contentArr === 'string') {
        texts = [contentArr];
      } else if (Array.isArray(contentArr)) {
        texts = contentArr
          .filter((b: { type: string }) => b.type === 'text')
          .map((b: { text: string }) => b.text || '');
      }

      let combined = texts.join(' ').trim();
      if (!combined) return;

      // Strip chorus prompt prefix — multiple formats:
      // "--- Silas | 2026-03-24 13:42 Boston | Werk v73 ---"
      // "--- Silas | 2026-03-24 13:42 Boston | #1665 | Werk v73 ---"
      combined = combined.replace(/^---\s+\w+\s+\|[^]*?---\s*/g, '').trim();
      if (!combined) return;

      // Filter role-to-role coordination from assistant output (#1675)
      if (combined.includes('[nudge from ')) return;
      if (combined.includes('role.nudge.consumed')) return;
      if (combined.match(/^\[(reply|ack|feedback|direction|correction)\]/i)) return;
      // Chat channel messages between roles
      if (combined.includes('chat.sh')) return;
      if (combined.match(/\[chat\]/i)) return;
      if (combined.match(/chat (file|channel|nudge|delivery|silas-kade|kade-silas|wren-silas|silas-wren|wren-kade|kade-wren)/i)) return;
      if (combined.match(/^nudge\.sh|^bash.*nudge\.sh/)) return;
      // Messaging tier coordination
      if (combined.match(/messaging (tier|API|api)/i) && !combined.includes('Jeff')) return;
      // Role-to-role delivery confirmations
      if (combined.match(/^DELIVERED to (wren|silas|kade)/i)) return;
      // Hook echo — bridge events reflected back through session
      if (combined.includes('[bridge]')) return;
      // Nudge delivery confirmations and receipts
      if (combined.match(/^(Delivered to|Nudged |Ack\b|Acknowledged\b|Test nudge|Copy\.|Copy,|Got it)/i)) return;
      if (combined.match(/DELIVERED to (wren|silas|kade)/i)) return;
      if (combined.match(/nudge(s|d)? (received|landed|confirmed|delivered|hit)/i)) return;
      if (combined.match(/^(Wren|Silas|Kade)'s (nudge|test|exchange|drain)/i)) return;
      if (combined.match(/^All (three|3) (directions|roles|nudges)/i)) return;
      if (combined.match(/^(Draining nudges|Nudge(s)? drained)/i)) return;
      if (combined.match(/reply expected/i)) return;
      // Pure coordination — not Jeff-facing
      if (combined.match(/^(Noted |Standing by|Test nudges|More test nudges|No action needed|Pipeline (solid|confirmed))/i)) return;
      // Spine event echoes — tailer.ts handles these as typed events (demo-ready, accept-request)
      if (combined.match(/^(Pulled #|Moved #|Demo ready:|Accepted #|card\.\w+)/)) return;
      // State declarations and board ops
      if (combined.match(/^(bash .*scripts\/|role-state\.sh|chorus-log\.sh)/)) return;

      // Debounce: replace pending message, emit after 3s quiet (#1720)
      // This ensures only the final response in a tool-call burst reaches Bridge
      const existing = this.pendingAssistant.get(role);
      if (existing) clearTimeout(existing.timer);

      const debounceTimer = setTimeout(() => {
        const pending = this.pendingAssistant.get(role);
        if (pending) {
          this.router.ingest({
            from: role,
            text: pending.text,
            ts: pending.ts,
            type: 'pm-thinking',
          });
          this.pendingAssistant.delete(role);
        }
      }, 3000);

      this.pendingAssistant.set(role, { text: combined, ts, timer: debounceTimer });
    }
    // All other types (tool_use, tool_result, system, progress) — filtered out
  }
}
