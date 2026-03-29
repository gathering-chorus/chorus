import { WebClient } from '@slack/web-api';
import * as fs from 'fs';
import * as path from 'path';
import { log } from './logger';

export const BRIDGE_MARKER = '··bridge';

/** Returns a role-tagged bridge marker, e.g. ··bridge:wren */
export function bridgeMarkerForRole(roleName: string): string {
  return `${BRIDGE_MARKER}:${roleName}`;
}

/** Checks if a message is from any bridge role */
export function isBridgeMessage(text: string): boolean {
  return text.includes(BRIDGE_MARKER);
}

/** Checks if a message is from a specific bridge role */
export function isBridgeMessageFromRole(text: string, roleName: string): boolean {
  return text.includes(bridgeMarkerForRole(roleName));
}

export interface SlackMessage {
  text: string;
  user: string;
  ts: string;
  thread_ts?: string;
  channel: string;
  channelName: string;
}

interface LastSeenState {
  [channelId: string]: string;
}

export class ChannelMonitor {
  private slack: WebClient;
  private botUserId: string | null = null;
  private channelMap: Map<string, string> = new Map(); // name -> id
  private lastSeen: LastSeenState = {};
  private statePath: string;

  constructor(slack: WebClient, statePath?: string) {
    this.slack = slack;
    this.statePath = statePath || path.resolve(__dirname, '../data/last-seen.json');
    this.loadState();
  }

  async initialize(): Promise<void> {
    // Get bot's own user ID for loop prevention
    const auth = await this.slack.auth.test();
    this.botUserId = auth.user_id as string;
    log('info', `Bot user ID: ${this.botUserId}`);

    // Build channel name -> ID map
    const result = await this.slack.conversations.list({ types: 'public_channel', limit: 200 });
    for (const channel of result.channels || []) {
      if (channel.name && channel.id) {
        this.channelMap.set(channel.name, channel.id);
      }
    }
    log('info', `Mapped ${this.channelMap.size} channels`);
  }

  getChannelId(name: string): string | undefined {
    return this.channelMap.get(name);
  }

  async poll(channelNames: string[]): Promise<SlackMessage[]> {
    const messages: SlackMessage[] = [];

    for (const name of channelNames) {
      const channelId = this.channelMap.get(name);
      if (!channelId) {
        log('warn', `Channel not found: ${name}`);
        continue;
      }

      try {
        const oldest = this.lastSeen[channelId] || '0';
        const result = await this.slack.conversations.history({
          channel: channelId,
          oldest,
          limit: 20,
        });

        for (const msg of result.messages || []) {
          // Skip bridge-originated responses (marker-based loop prevention)
          if (msg.text?.includes(BRIDGE_MARKER)) continue;
          // Skip messages without text
          if (!msg.text) continue;
          // Skip messages at or before last seen timestamp
          if (msg.ts && msg.ts <= oldest) continue;

          messages.push({
            text: msg.text,
            user: msg.user || 'unknown',
            ts: msg.ts || '0',
            thread_ts: msg.thread_ts,
            channel: channelId,
            channelName: name,
          });
        }

        // Update last seen to the newest message timestamp
        if (result.messages && result.messages.length > 0) {
          const newest = result.messages[0]?.ts;
          if (newest && (!this.lastSeen[channelId] || newest > this.lastSeen[channelId])) {
            this.lastSeen[channelId] = newest;
          }
        }
      } catch (err) {
        log('error', `Failed to poll channel ${name}: ${err}`);
      }
    }

    this.saveState();
    return messages;
  }

  /**
   * Get recent messages for group conversation context.
   * Includes other roles' bridge messages but excludes the specified role's own output.
   * This allows each role to see what other roles said during a group conversation.
   */
  async getRecentMessagesForGroup(
    channelName: string,
    excludeRole: string,
    limit = 20
  ): Promise<SlackMessage[]> {
    const channelId = this.channelMap.get(channelName);
    if (!channelId) return [];

    try {
      const result = await this.slack.conversations.history({
        channel: channelId,
        limit,
      });

      const messages: SlackMessage[] = [];
      for (const msg of (result.messages || []).reverse()) {
        if (!msg.text || !msg.ts) continue;
        // Skip this role's own bridge messages
        if (msg.text.includes(bridgeMarkerForRole(excludeRole))) continue;
        messages.push({
          text: msg.text,
          user: msg.user || 'unknown',
          ts: msg.ts,
          thread_ts: msg.thread_ts,
          channel: channelId,
          channelName,
        });
      }
      return messages;
    } catch (err) {
      log('error', `Failed to get group messages for ${channelName}: ${err}`);
      return [];
    }
  }

  /**
   * Check if a human posted in the channel since a given timestamp.
   * Used by the group orchestrator to detect moderation (Jeff intervening).
   * Returns the human message if found, null otherwise.
   */
  async checkForHumanMessage(
    channelName: string,
    sinceTs: string
  ): Promise<SlackMessage | null> {
    const channelId = this.channelMap.get(channelName);
    if (!channelId) return null;

    try {
      const result = await this.slack.conversations.history({
        channel: channelId,
        oldest: sinceTs,
        limit: 10,
      });

      for (const msg of result.messages || []) {
        if (!msg.text || !msg.ts) continue;
        // Skip bridge messages — we only want human messages
        if (msg.text.includes(BRIDGE_MARKER)) continue;
        // Skip the trigger message itself
        if (msg.ts === sinceTs) continue;
        // Found a human message posted after sinceTs
        return {
          text: msg.text,
          user: msg.user || 'unknown',
          ts: msg.ts,
          channel: channelId,
          channelName,
        };
      }
    } catch (err) {
      log('error', `Failed to check for human messages: ${err}`);
    }
    return null;
  }

  private loadState(): void {
    try {
      if (fs.existsSync(this.statePath)) {
        const raw = fs.readFileSync(this.statePath, 'utf-8');
        this.lastSeen = JSON.parse(raw);
        log('info', `Loaded last-seen state for ${Object.keys(this.lastSeen).length} channels`);
      }
    } catch {
      log('warn', 'Could not load last-seen state, starting fresh');
      this.lastSeen = {};
    }
  }

  private saveState(): void {
    try {
      const dir = path.dirname(this.statePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.statePath, JSON.stringify(this.lastSeen, null, 2));
    } catch (err) {
      log('error', `Failed to save last-seen state: ${err}`);
    }
  }
}
