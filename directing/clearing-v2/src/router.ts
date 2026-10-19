// @domain: messages

import { ClearingMessage, PulseNudgePayload } from './types';

const KNOWN_ROLES: Set<string> = new Set(['jeff', 'wren', 'silas', 'kade', 'abby-normal']);

/** #4575 — strip the Chorus Prompt header ("--- Role | ... ---") so a role's signed reply
 *  and v1's bridge-signed mirror share one clean body and match as the same message. */
export function stripPromptHeader(text: string): string {
  return (text || '').replace(/^---\s+[^|\n]+\|[^\n]+---\s*\n*/, '');
}

export class MessageRouter {
  private messages: ClearingMessage[] = [];
  private seenIds: Set<string> = new Set();
  private maxMessages: number;

  constructor(maxMessages = 200) {
    this.maxMessages = maxMessages;
  }

  private trimIfFull(): void {
    while (this.messages.length > this.maxMessages) {
      const removed = this.messages.shift();
      if (removed && this.seenIds.size > this.maxMessages * 5) {
        this.seenIds.delete(removed.id);
      }
    }
  }

  private findSameBody(room: string, body: string, filter?: (m: ClearingMessage) => boolean): ClearingMessage | undefined {
    if (!body) return undefined;
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const m = this.messages.at(i);
      if (!m) continue;
      if ((m.room || 'team') !== room) continue;
      if (filter && !filter(m)) continue;
      if (m.text.trim() === body) return m;
    }
    return undefined;
  }

  /**
   * Ingest a message into the router.
   * Returns true if newly ingested (or upgraded from a bridge copy), or false if duplicate/dropped.
   */
  ingest(msg: ClearingMessage): boolean {
    if (!msg.id || this.seenIds.has(msg.id)) return false;

    const cleanText = stripPromptHeader(msg.text);
    const body = cleanText.trim();
    const room = msg.room || 'team';

    if (msg.from === 'bridge' && this.findSameBody(room, body)) {
      this.seenIds.add(msg.id);
      return false;
    }

    // #4591 — a bridge-signed mirror whose author was unwrapped to msg.from is an echo if the room already holds it
    if (msg.signer === 'bridge' && this.findSameBody(room, body, (m) => m.from === msg.from)) {
      this.seenIds.add(msg.id);
      return false;
    }

    if (msg.signer !== 'bridge' && msg.from !== 'bridge') {
      const bridgeCopy = this.findSameBody(room, body, (m) => m.from === 'bridge' || (m.from === msg.from && m.signer === 'bridge'));
      if (bridgeCopy) {
        this.seenIds.add(msg.id);
        Object.assign(bridgeCopy, { ...msg, text: cleanText });
        this.messages.sort((a, b) => (a.ts || '').localeCompare(b.ts || '') || a.id.localeCompare(b.id));
        return true;
      }
    }

    this.seenIds.add(msg.id);
    this.messages.push({ ...msg, text: cleanText });
    this.messages.sort((a, b) => (a.ts || '').localeCompare(b.ts || '') || a.id.localeCompare(b.id));
    this.trimIfFull();
    return true;
  }

  /**
   * Get recent visible messages, optionally filtered by room, newest `limit` in chronological order.
   */
  getRecent(limit = 100, room?: string): ClearingMessage[] {
    return this.messages
      .filter((m) => {
        if (!m.visible) return false;
        if (!room) return true;
        const msgRoom = m.room || 'team';
        return msgRoom === room;
      })
      .slice(-limit);
  }

  /**
   * #4597 — the page of visible messages just older than `beforeId` in a room, oldest first,
   * so scrolling up in the room loads history one page at a time. Unknown id → none.
   */
  getBefore(beforeId: string, limit: number, room = 'team'): ClearingMessage[] {
    const inRoom = this.messages.filter((m) => m.visible && (m.room || 'team') === room);
    const idx = inRoom.findIndex((m) => m.id === beforeId);
    if (idx <= 0) return [];
    return inRoom.slice(Math.max(0, idx - limit), idx);
  }

  /**
   * Extract @role mentions to generate Pulse nudge dispatch payloads.
   */
  extractNudges(msg: ClearingMessage): PulseNudgePayload[] {
    if (!msg.text) return [];

    const mentionRegex = /@([a-z0-9-]+)/gi;
    const matches = Array.from(msg.text.matchAll(mentionRegex));
    const nudges: PulseNudgePayload[] = [];
    const seenRoles = new Set<string>();

    for (const match of matches) {
      const targetRole = match[1].toLowerCase();
      if (KNOWN_ROLES.has(targetRole) && targetRole !== msg.from && !seenRoles.has(targetRole)) {
        seenRoles.add(targetRole);
        nudges.push({
          from: msg.from,
          to: targetRole,
          content: msg.text,
        });
      }
    }

    // #4604: unaddressed Jeff input routes to every role as in v1 (#3833).
    if (nudges.length === 0 && msg.from === 'jeff') {
      for (const role of KNOWN_ROLES) {
        if (role !== 'jeff') {
          nudges.push({
            from: 'jeff',
            to: role,
            content: msg.text,
          });
        }
      }
    }

    return nudges;
  }

  /**
   * Get all distinct room names from ingested messages.
   */
  getKnownRooms(): string[] {
    const rooms = new Set<string>();
    for (const m of this.messages) {
      if (m.room) rooms.add(m.room);
    }
    return Array.from(rooms);
  }
}
