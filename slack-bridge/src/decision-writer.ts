import * as fs from 'fs';
import * as path from 'path';
import { SlackMessage } from './channel-monitor';
import { log } from './logger';

/**
 * Path to the decisions backlog inside the container.
 * Mounted from ../../messages/decisions via docker-compose.yml.
 */
const DECISIONS_DIR = '/team/decisions';
const BACKLOG_FILE = path.join(DECISIONS_DIR, 'backlog.md');

/**
 * Detect whether a message contains a [DECISION] tag (case-insensitive).
 */
export function isDecisionMessage(text: string): boolean {
  return /\[decision\]/i.test(text);
}

/**
 * Append a decision entry to the backlog file.
 * Called from commands.ts when a [DECISION] tag is detected.
 */
export function writeDecision(msg: SlackMessage): void {
  const now = new Date();
  const dateStr = now.toISOString().split('T')[0];
  const timeStr = now.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });

  const entry = [
    `## [DECISION] ${dateStr} ${timeStr} — #${msg.channelName}`,
    `**From:** ${msg.user}`,
    `**Text:** ${msg.text}`,
    `**Status:** pending`,
    '',
    '---',
    '',
  ].join('\n');

  try {
    // Ensure directory exists
    if (!fs.existsSync(DECISIONS_DIR)) {
      fs.mkdirSync(DECISIONS_DIR, { recursive: true });
    }

    // Create file with header if it doesn't exist
    if (!fs.existsSync(BACKLOG_FILE)) {
      fs.writeFileSync(
        BACKLOG_FILE,
        '# Decisions Backlog\n\nCaptured automatically by the bridge when `[DECISION]` tags appear in Slack.\nRoles process entries on session start and update status to `processed-by-{role}`.\n\n---\n\n',
        'utf-8',
      );
    }

    fs.appendFileSync(BACKLOG_FILE, entry, 'utf-8');
    log('info', `Decision captured from #${msg.channelName}`, {
      event: 'decision_captured',
      channel: msg.channelName,
      user: msg.user,
    });
  } catch (err) {
    log('error', `Failed to write decision: ${err}`, {
      channel: msg.channelName,
    });
  }
}
