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

// #2167: env-configurable so tests can point at a fixture directory.
const PROJECTS_DIR = process.env.CLEARING_PROJECTS_DIR || '/Users/jeffbridwell/.claude/projects';
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
          } catch { /* ignored */ }
          this.sessions.set(role, state);
        } catch { /* ignored */ }
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
          } catch { /* ignored */ }
        }
      }

      return newest ? newest.path : null;
    } catch { /* ignored */ }
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
          } catch { /* ignored */ }
          this.sessions.set(role, newState);
        } catch { continue; }
      }

      // Fallback read for anything fs.watch missed
      this.readNewEntries(role);
    }
  }

  private extractUserText(rawContent: unknown): string {
    if (typeof rawContent === 'string') return rawContent.trim();
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

  private handleUserMessage(entry: any, ts: string): void {
    const rawContent = entry.message?.content;
    if (!rawContent) return;
    let text = this.extractUserText(rawContent);
    if (!text) return;
    text = text.replace(/\n/g, ' ');

    const nudgeMatch = text.match(/^\[nudge from (wren|silas|kade)/i);
    if (nudgeMatch) {
      this.router.ingest({ from: nudgeMatch[1].toLowerCase(), text, ts, type: 'role-response' });
    } else {
      this.router.ingest({ from: 'jeff', text, ts, type: 'jeff-input' });
    }
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

  private handleAssistantMessage(role: string, entry: any, ts: string): void {
    const contentArr = entry.message?.content;
    if (!contentArr) return;
    let combined = this.extractAssistantText(contentArr);
    if (!combined) return;
    combined = combined.replace(/^---\s+\w+\s+\|[^]*?---\s*/g, '').trim();
    if (!combined) return;
    if (this.isFilteredAssistantText(combined)) return;

    const existing = this.pendingAssistant.get(role);
    if (existing) clearTimeout(existing.timer);
    const debounceTimer = setTimeout(() => {
      const pending = this.pendingAssistant.get(role);
      if (pending) {
        this.router.ingest({ from: role, text: pending.text, ts: pending.ts, type: 'pm-thinking' });
        this.pendingAssistant.delete(role);
      }
    }, 3000);
    this.pendingAssistant.set(role, { text: combined, ts, timer: debounceTimer });
  }

  private processLine(role: string, line: string): void {
    let entry: any;
    try { entry = JSON.parse(line); } catch { return; }
    const ts = entry.timestamp || new Date().toISOString();
    if (entry.type === 'user') return this.handleUserMessage(entry, ts);
    if (entry.type === 'assistant') return this.handleAssistantMessage(role, entry, ts);
  }
}
