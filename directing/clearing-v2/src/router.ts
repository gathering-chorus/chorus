// @domain: messages

import { ClearingMessage, PulseNudgePayload } from './types';

const KNOWN_ROLES: Set<string> = new Set(['jeff', 'wren', 'silas', 'kade', 'abby-normal']);

export class MessageRouter {
  private messages: ClearingMessage[] = [];
  private seenIds: Set<string> = new Set();
  private maxMessages: number;

  constructor(maxMessages = 200) {
    this.maxMessages = maxMessages;
  }

  /**
   * Ingest a message into the router.
   * Returns true if newly ingested, or false if duplicate/dropped.
   */
  ingest(msg: ClearingMessage): boolean {
    if (!msg.id) return false;
    if (this.seenIds.has(msg.id)) {
      return false;
    }

    this.seenIds.add(msg.id);
    this.messages.push(msg);

    if (this.messages.length > this.maxMessages) {
      const removed = this.messages.shift();
      if (removed) {
        // Keep seenIds bounded to prevent unbounded memory growth
        if (this.seenIds.size > this.maxMessages * 5) {
          this.seenIds.delete(removed.id);
        }
      }
    }

    return true;
  }

  /**
   * Get recent visible messages.
   */
  getRecent(limit = 50): ClearingMessage[] {
    return this.messages.filter((m) => m.visible).slice(-limit);
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

    return nudges;
  }
}
