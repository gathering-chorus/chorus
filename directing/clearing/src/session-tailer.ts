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
  wren: 'wren',
  silas: 'silas',
  kade: 'kade',
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

    // Find the newest .jsonl across ALL matching project dirs (#2035)
    // Multiple dirs can match (e.g. "architect" and "chorus-architect")
    try {
      const entries = fs.readdirSync(PROJECTS_DIR);
      let newest: { path: string; mtime: number } | null = null;

      for (const entry of entries) {
        if (entry.includes(roleDir)) {
          const projDir = path.join(PROJECTS_DIR, entry);
          try {
            const files = fs.readdirSync(projDir)
              .filter(f => f.endsWith('.jsonl'))
              .map(f => {
                const fullPath = path.join(projDir, f);
                return { path: fullPath, mtime: fs.statSync(fullPath).mtimeMs };
              });
            for (const file of files) {
              if (!newest || file.mtime > newest.mtime) {
                newest = file;
              }
            }
          } catch {}
        }
      }

      return newest ? newest.path : null;
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

      // Filter system artifacts only — never filter Jeff's words by content (#2035)
      // System-reminder and command expansions already filtered at line 180-183
      if (/<system-reminder>/i.test(text)) return;
      if (/<command-/i.test(text)) return;
      if (text.includes('[Image: source:')) return;
      if (text.includes('[Request interrupted')) return;
      if (text.startsWith('ARGUMENTS:') || text.startsWith('Base directory') || text.startsWith('Stop hook')) return;

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

      // Filter machine protocol only — let human-readable role thinking through (#2035)
      // The router's pm-thinking classification handles visibility at display time
      // Keep filtering: raw spine events, state declarations, delivery confirmations, script calls
      if (combined.match(/^DELIVERED to (wren|silas|kade)/i)) return;
      if (combined.match(/^card\.\w+/)) return;
      if (combined.match(/^(bash .*scripts\/|role-state |chorus-log )/)) return;
      if (combined.includes('[bridge]')) return;
      if (combined.includes('role.nudge.consumed')) return;

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
