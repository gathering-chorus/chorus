/**
 * Chat module for The Clearing — merged from clearing/src/server.ts
 * Adds multi-party AI chat as a mode within the Bridge dashboard.
 *
 * #1795: The Clearing = one URL, one service, two modes (dashboard + chat).
 */

import { Server as SocketServer } from 'socket.io';
import { Participants } from './participants';
import { Transcript, ChatMessage } from './transcript';

const MODEL = process.env.CLEARING_MODEL || 'claude-haiku-4-5-20251001';
const MAX_TOKENS = parseInt(process.env.CLEARING_MAX_TOKENS || '300');
const CHORUS_ROOT = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects';
const NUDGE_BINARY = `${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim`;

function executeNudge(from: string, target: string, message: string): { success: boolean; detail: string } {
  const { execSync } = require('child_process');
  try {
    const safeMsg = message.replace(/"/g, '\\"');
    const result = execSync(
      `${NUDGE_BINARY} nudge ${target} "${safeMsg}" --from ${from}`,
      { timeout: 10_000, encoding: 'utf-8' }
    ).trim();
    return { success: true, detail: result };
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    return { success: false, detail: errMsg };
  }
}

function extractNudges(content: string): { cleaned: string; nudges: Array<{ target: string; message: string }> } {
  const nudges: Array<{ target: string; message: string }> = [];
  const cleaned = content.replace(/^\/nudge\s+(wren|silas|kade)\s+(.+)$/gim, (_match, target, msg) => {
    nudges.push({ target: target.toLowerCase(), message: msg.replace(/^["']|["']$/g, '') });
    return '';
  });
  return { cleaned: cleaned.trim(), nudges };
}

function parseAddressed(content: string): string[] {
  const mentions = content.match(/@(\w+)/g);
  if (!mentions) return [];
  return mentions.map((m) => m.slice(1).toLowerCase());
}

export class ClearingChat {
  private participants: Participants;
  private transcript: Transcript;
  private sessionActive = false;
  private autoSaveInterval: NodeJS.Timeout | null = null;
  private sessionId = '';

  constructor(private io: SocketServer, context?: string) {
    this.participants = new Participants(MODEL, MAX_TOKENS, context || '', false);
    this.transcript = new Transcript(MODEL);
  }

  /** Start a new chat session */
  startSession(context?: string): { sessionId: string } {
    if (this.sessionActive && this.transcript.getMessages().length > 0) {
      this.endSession('new-session');
    }
    this.sessionId = `clearing-${Date.now()}`;
    this.sessionActive = true;
    if (context) {
      this.participants.updateContext(context);
    }
    this.startAutoSave();
    console.log(`[chat] New session: ${this.sessionId}`);
    return { sessionId: this.sessionId };
  }

  /** End current session, abort in-flight API streams, save transcript */
  endSession(reason: string): void {
    if (!this.sessionActive) return;
    this.stopAutoSave();
    this.sessionActive = false;
    this.participants.abort();
    const label = reason === 'client-disconnected' ? 'host disconnected' : reason;
    console.log(`[chat] Clearing session ended — ${label}`);

    const msgs = this.transcript.getMessages();
    if (msgs.length > 0) {
      this.transcript.save();
      const decisions = this.transcript.extractDecisions();
      if (decisions.length > 0) {
        console.log(`[chat] Session ended (${reason}): ${msgs.length} messages, ${decisions.length} decisions`);
      } else {
        console.log(`[chat] Session ended (${reason}): ${msgs.length} messages`);
      }
    }
    // Reset for next session
    this.transcript = new Transcript(MODEL);
  }

  private tryHandleNudgeCommand(content: string): boolean {
    const shorthandMap: Record<string, string> = { nw: 'wren', ns: 'silas', nk: 'kade' };
    const nudgeMatch = content.match(/^(?:\/nudge\s+(wren|silas|kade)|(nw|ns|nk))\s+(.+)$/i);
    if (!nudgeMatch) return false;
    const target = (nudgeMatch[1] || shorthandMap[nudgeMatch[2].toLowerCase()]).toLowerCase();
    const msg = nudgeMatch[3].replace(/^["']|["']$/g, '');
    const jeffMsg = this.transcript.add('Jeff', content);
    this.io.emit('chat:message', jeffMsg);
    const result = executeNudge('jeff', target, msg);
    const sysMsg = this.transcript.add('System',
      result.success ? `Nudge sent to ${target}.` : `Nudge to ${target} failed: ${result.detail}`);
    this.io.emit('chat:message', sysMsg);
    return true;
  }

  private selectRespondingRoles(content: string, activeRoles?: string[]) {
    const addressed = parseAddressed(content);
    const all = this.participants.getRoles();
    if (addressed.length > 0) return all.filter((r) => addressed.includes(r.name.toLowerCase()));
    if (activeRoles && activeRoles.length > 0) return all.filter((r) => activeRoles.includes(r.name.toLowerCase()));
    return all;
  }

  private async runRoleResponse(role: { name: string }): Promise<void> {
    try {
      this.io.emit('chat:typing', { sender: role.name });
      const response = await this.participants.getResponse(
        role as Parameters<typeof this.participants.getResponse>[0],
        this.transcript.getMessages(),
        (token: string) => { this.io.emit('chat:token', { sender: role.name, token }); }
      );
      const trimmed = response.content.trim();
      if (trimmed === '[pass]' || trimmed.toLowerCase().startsWith('[pass]')) {
        this.io.emit('chat:typed', { sender: role.name, passed: true });
        return;
      }
      const { cleaned, nudges } = extractNudges(response.content);
      const displayContent = nudges.length > 0 ? cleaned : response.content;
      const roleMsg = this.transcript.add(role.name, displayContent, {
        input: response.inputTokens,
        output: response.outputTokens,
      });
      this.io.emit('chat:typed', { sender: role.name, message: roleMsg });
      for (const nudge of nudges) {
        const result = executeNudge(role.name.toLowerCase(), nudge.target, nudge.message);
        const notice = this.transcript.add('System',
          result.success
            ? `${role.name} nudged ${nudge.target}: "${nudge.message}"`
            : `${role.name}'s nudge to ${nudge.target} failed: ${result.detail}`
        );
        this.io.emit('chat:message', notice);
      }
      this.io.emit('chat:cost', {
        totalTokens: this.transcript.getTotalTokens(),
        estimatedCost: this.transcript.getEstimatedCost(),
        messageCount: this.transcript.getMessages().length,
      });
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      const sysMsg = this.transcript.add('System', `${role.name} failed: ${errMsg}`);
      this.io.emit('chat:typed', { sender: role.name, message: sysMsg });
    }
  }

  /** Handle incoming message from Jeff */
  async handleMessage(content: string, activeRoles?: string[]): Promise<void> {
    if (this.tryHandleNudgeCommand(content)) return;

    const userMsg = this.transcript.add('Jeff', content);
    this.io.emit('chat:message', userMsg);

    if (/^DECISION[\s:–—-]/i.test(content)) {
      this.io.emit('chat:decision', { messageId: userMsg.id, text: content, speaker: 'Jeff' });
    }

    for (const role of this.selectRespondingRoles(content, activeRoles)) {
      await this.runRoleResponse(role);
    }
  }

  /** Get session state */
  getState() {
    return {
      active: this.sessionActive,
      sessionId: this.sessionId,
      messageCount: this.transcript.getMessages().length,
      roles: this.participants.getRoles().map(r => ({ name: r.name, title: r.title, color: r.color })),
    };
  }

  /** Get messages since a given ID */
  getMessages(sinceId?: number): ChatMessage[] {
    const msgs = this.transcript.getMessages();
    if (!sinceId) return msgs;
    return msgs.filter(m => parseInt(m.id) > sinceId);
  }

  private startAutoSave() {
    if (this.autoSaveInterval) return;
    this.autoSaveInterval = setInterval(() => {
      if (this.transcript.getMessages().length > 0) {
        this.transcript.save();
      }
    }, 30_000);
  }

  private stopAutoSave() {
    if (this.autoSaveInterval) {
      clearInterval(this.autoSaveInterval);
      this.autoSaveInterval = null;
    }
  }
}
