import * as fs from 'fs';
import * as path from 'path';
import { SlackMessage } from './channel-monitor';
import { ResponsePoster } from './response-poster';
import { RoleConfig } from './config';
import { isDecisionMessage, writeDecision } from './decision-writer';
import { log } from './logger';

/**
 * Bridge commands — detected before routing, handled without Claude calls.
 * Returns true if the message was handled as a command.
 */
export async function handleCommand(
  msg: SlackMessage,
  poster: ResponsePoster,
  roles: RoleConfig[]
): Promise<boolean> {
  const text = msg.text.toLowerCase().trim();

  if (text.includes('@read-briefs')) {
    await handleReadBriefs(msg, poster, roles);
    return true;
  }

  // Capture [DECISION] tagged messages to the decisions backlog
  if (isDecisionMessage(msg.text)) {
    writeDecision(msg);
    // Don't return true — let the message also route to roles for normal processing
  }

  return false;
}

/**
 * @read-briefs — scan each role's briefs directory and post a summary.
 * Shows only actionable items: commitment briefs from conversations
 * and any new briefs from the last 48 hours.
 */
async function handleReadBriefs(
  msg: SlackMessage,
  poster: ResponsePoster,
  roles: RoleConfig[]
): Promise<void> {
  log('info', '@read-briefs command received');

  const lines: string[] = [];
  const twoDaysAgo = Date.now() - 48 * 60 * 60 * 1000;

  for (const role of roles) {
    const briefs = listActionableBriefs(role.briefsPath, twoDaysAgo);
    const displayName = role.name.charAt(0).toUpperCase() + role.name.slice(1);

    if (briefs.length === 0) {
      lines.push(`**${displayName}**: no new briefs`);
    } else {
      lines.push(`**${displayName}** (${briefs.length}):`);
      for (const b of briefs) {
        const tag = b.isCommitment ? ':speech_balloon:' : ':page_facing_up:';
        lines.push(`  ${tag} ${b.shortName} _(${b.age})_`);
      }
    }
  }

  const summary = lines.join('\n');
  await poster.postSystem(msg.channel, summary, msg.thread_ts);
}

interface BriefInfo {
  shortName: string;
  age: string;
  isCommitment: boolean;
}

function listActionableBriefs(briefsPath: string, since: number): BriefInfo[] {
  try {
    if (!fs.existsSync(briefsPath)) return [];
    return fs.readdirSync(briefsPath)
      .filter(f => f.endsWith('.md'))
      .map(f => {
        const stat = fs.statSync(path.join(briefsPath, f));
        return { name: f, mtime: stat.mtimeMs };
      })
      .filter(f => f.mtime >= since)
      .sort((a, b) => b.mtime - a.mtime)
      .map(({ name, mtime }) => ({
        shortName: humanizeBriefName(name),
        age: timeAgo(mtime),
        isCommitment: name.includes('conversation-commitment'),
      }));
  } catch (err) {
    log('warn', `Could not list briefs at ${briefsPath}: ${err}`);
    return [];
  }
}

/** Convert filename to readable label: 2026-02-20-conversation-commitment-0225.md → "Conversation commitment" */
function humanizeBriefName(filename: string): string {
  return filename
    .replace(/^\d{4}-\d{2}-\d{2}-/, '')  // strip date prefix
    .replace(/\.md$/, '')                  // strip extension
    .replace(/-\d{4}$/, '')               // strip time suffix (e.g., -0225)
    .replace(/-/g, ' ')                    // hyphens to spaces
    .replace(/^./, c => c.toUpperCase());  // capitalize first letter
}

/** Convert timestamp to human-readable age: "2h ago", "yesterday" */
function timeAgo(mtime: number): string {
  const hours = Math.round((Date.now() - mtime) / (1000 * 60 * 60));
  if (hours < 1) return 'just now';
  if (hours === 1) return '1h ago';
  if (hours < 24) return `${hours}h ago`;
  if (hours < 48) return 'yesterday';
  return `${Math.round(hours / 24)}d ago`;
}
