import { WebClient } from '@slack/web-api';
import { log } from './logger';
import { metrics } from './metrics';
import { BRIDGE_MARKER, bridgeMarkerForRole } from './channel-monitor';

const MAX_MESSAGE_LENGTH = 3000;
const MAX_GROUP_WORDS = 75; // ~5 sentences, hard cap for group conversations

export class ResponsePoster {
  private slack: WebClient;

  constructor(slack: WebClient) {
    this.slack = slack;
  }

  async post(
    channel: string,
    roleName: string,
    text: string,
    threadTs?: string
  ): Promise<void> {
    const formatted = `**${capitalize(roleName)}**: ${text}\n${bridgeMarkerForRole(roleName)}`;
    const chunks = splitMessage(formatted, MAX_MESSAGE_LENGTH);

    for (const chunk of chunks) {
      try {
        await this.slack.chat.postMessage({
          channel,
          text: chunk,
          thread_ts: threadTs,
          unfurl_links: false,
          unfurl_media: false,
        });
        metrics.responsesSent.inc({ channel, role: roleName });
      } catch (err) {
        metrics.errors.inc({ type: 'slack_post' });
        log('error', `Failed to post to ${channel}: ${err}`);
        throw err;
      }
    }
  }

  async postSystem(channel: string, text: string, threadTs?: string): Promise<void> {
    const formatted = `${text}\n${BRIDGE_MARKER}`;
    try {
      await this.slack.chat.postMessage({
        channel,
        text: formatted,
        thread_ts: threadTs,
        unfurl_links: false,
        unfurl_media: false,
      });
    } catch (err) {
      log('error', `Failed to post system message: ${err}`);
    }
  }

  async postRateLimited(channel: string, roleName: string, threadTs?: string): Promise<void> {
    const text = `**${capitalize(roleName)}**: I've hit my rate limit for this hour. I'll be back shortly.\n${bridgeMarkerForRole(roleName)}`;
    try {
      await this.slack.chat.postMessage({
        channel,
        text,
        thread_ts: threadTs,
        unfurl_links: false,
        unfurl_media: false,
      });
    } catch (err) {
      log('error', `Failed to post rate limit notice: ${err}`);
    }
  }
}

/**
 * Hard word-count cap for group conversation responses.
 * If the model exceeds the limit despite prompt instructions, truncate at sentence boundary.
 */
export function truncateForGroup(text: string, maxWords = MAX_GROUP_WORDS): string {
  const words = text.split(/\s+/);
  if (words.length <= maxWords) return text;

  // Find the last sentence boundary within the word limit
  const truncated = words.slice(0, maxWords).join(' ');
  const lastSentence = truncated.search(/[.!?][^.!?]*$/);
  if (lastSentence > truncated.length / 2) {
    return truncated.slice(0, lastSentence + 1);
  }
  return truncated + '...';
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function splitMessage(text: string, maxLen: number): string[] {
  if (text.length <= maxLen) return [text];

  const chunks: string[] = [];
  let remaining = text;

  while (remaining.length > 0) {
    if (remaining.length <= maxLen) {
      chunks.push(remaining);
      break;
    }

    // Try to split at a newline
    let splitIdx = remaining.lastIndexOf('\n', maxLen);
    if (splitIdx < maxLen / 2) {
      // No good newline, split at space
      splitIdx = remaining.lastIndexOf(' ', maxLen);
    }
    if (splitIdx < maxLen / 2) {
      // No good space either, hard split
      splitIdx = maxLen;
    }

    chunks.push(remaining.slice(0, splitIdx));
    remaining = remaining.slice(splitIdx).trimStart();
  }

  return chunks;
}
