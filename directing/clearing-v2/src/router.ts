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
    if (this.messages.length <= this.maxMessages) return;
    const removed = this.messages.shift();
    if (removed && this.seenIds.size > this.maxMessages * 5) {
      this.seenIds.delete(removed.id);
    }
  }

  private findSameBody(room: string, body: string, fromFilter?: (from: string) => boolean): ClearingMessage | undefined {
    if (!body) return undefined;
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const m = this.messages.at(i);
      if (!m) continue;
      if ((m.room || 'team') !== room) continue;
      if (fromFilter && !fromFilter(m.from)) continue;
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

    if (msg.from !== 'bridge') {
      const bridgeCopy = this.findSameBody(room, body, (f) => f === 'bridge');
      if (bridgeCopy) {
        this.seenIds.add(msg.id);
        Object.assign(bridgeCopy, { ...msg, text: cleanText });
        return true;
      }
    }

    this.seenIds.add(msg.id);
    this.messages.push({ ...msg, text: cleanText });
    this.trimIfFull();
    return true;
  }

  /**
   * Get recent visible messages, optionally filtered by room.
   */
  getRecent(limit = 50, room?: string): ClearingMessage[] {
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

    // #4571 — v1 rule ("no @ = Wren gets it"): unaddressed Jeff input routes to wren.
    if (nudges.length === 0 && msg.from === 'jeff') {
      nudges.push({
        from: 'jeff',
        to: 'wren',
        content: msg.text,
      });
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
