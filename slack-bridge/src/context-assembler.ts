import * as fs from 'fs';
import * as path from 'path';
import { RoleConfig } from './config';
import { log } from './logger';

const BRIDGE_PREAMBLE = `You are responding via Slack, not in a Claude Code session.
You CAN: answer questions, discuss, acknowledge, summarize, reference files you've read.
You CANNOT: write files, run commands, make commits, or change system state.
If a request requires those capabilities, say: "That needs a Claude Code session — I can discuss it here but can't write files or run commands from Slack."
You are chatting on Slack. Be concise. Keep responses under ~300 words unless the question genuinely requires more.
No headers, tables, or multi-section formatting. Conversational tone — thinking at a whiteboard, not writing a brief.
If your answer would exceed a few paragraphs, say you'll write a brief instead.`;

const GROUP_PREAMBLE = `You are in a GROUP CONVERSATION on Slack. This is a real meeting — not a writing exercise.

HARD RULES (violating these is a failure):
1. MAX 3-5 SENTENCES. Not paragraphs. Sentences. Like a person talking in a meeting.
2. NO HEADERS. NO BULLET LISTS. NO SECTIONS. NO MARKDOWN FORMATTING. Write plain text like you're speaking.
3. LISTEN FIRST. If others have spoken, your FIRST sentence must reflect what you heard — agree, disagree, or build on a specific point they made. Do not just launch into your own take.
4. ONE IDEA per turn. Not three. Not a framework. One concrete thought from your perspective.
5. You CANNOT write files, run commands, or change system state from Slack.

You are in a meeting room. Jeff is moderating. Speak when spoken to, say your piece, then stop. Leave room for others.
6. NEVER assign decision numbers (DEC-XXX). You don't have the current state. Just say "that's a decision" and the role will capture it in their Claude session.`;

export interface ConversationTurn {
  roleName: string;
  text: string;
}

/**
 * Patterns to scrub from context before sending to Anthropic API.
 * Union of Wren's data-scrubbing brief + Silas's boundary-enforcement brief.
 * Each entry: [label for REDACTED tag, regex pattern].
 */
const SCRUB_PATTERNS: Array<[string, RegExp]> = [
  // Network identifiers
  ['ipv4', /\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/g],
  ['ipv6', /\b([0-9a-fA-F]{1,4}:){2,7}[0-9a-fA-F]{1,4}\b/g],
  ['mac', /\b([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b/g],
  ['port', /\blocalhost:\d{4,5}\b/g],

  // Credentials and tokens
  ['aws-key', /\bAKIA[A-Z0-9]{16}\b/g],
  ['slack-token', /\bxox[bpras]-[A-Za-z0-9\-]{10,}/g],
  ['github-token', /\bghp_[A-Za-z0-9]{36}\b/g],
  ['openai-token', /\bsk-[A-Za-z0-9]{20,}/g],
  ['secret-value', /\b(password|token|secret|api_key|apikey|auth_token)\s*[=:]\s*\S+/gi],

  // Infrastructure identifiers
  ['container-id', /\b[a-f0-9]{12,64}\b(?=\s|$|[,;)\]}])/g],
];

export class ContextAssembler {
  private activityPath: string;

  constructor(activityPath?: string) {
    this.activityPath = activityPath || '/team/messages/activity.md';
  }

  /**
   * Scrub sensitive patterns from text before sending to Anthropic API.
   * Returns scrubbed text and logs each pattern match for audit.
   */
  scrubContext(text: string, source: string): string {
    let scrubbed = text;
    for (const [label, pattern] of SCRUB_PATTERNS) {
      // Reset regex state (global flag)
      pattern.lastIndex = 0;
      const matches = scrubbed.match(pattern);
      if (matches && matches.length > 0) {
        log('info', 'context_scrub', {
          event: 'context_scrub',
          pattern: label,
          source,
          count: matches.length,
        });
        scrubbed = scrubbed.replace(pattern, `[REDACTED:${label}]`);
      }
    }
    return scrubbed;
  }

  /**
   * Assemble context for a group conversation turn.
   * Includes prior turns so each role can see what others said.
   */
  assembleForGroup(
    role: RoleConfig,
    priorTurns: ConversationTurn[],
    triggerMessage: string,
    recentChannelHistory: string[]
  ): string {
    const parts: string[] = [GROUP_PREAMBLE, ''];

    // Role identity (scrub each source independently)
    const claudeMd = this.readFileSafe(role.claudeMdPath);
    if (claudeMd) {
      parts.push('--- ROLE IDENTITY ---', this.scrubContext(claudeMd, 'role_identity'), '');
    }

    // Memory
    const memory = this.readFileSafe(role.memoryPath);
    if (memory) {
      parts.push('--- MEMORY ---', this.scrubContext(memory, 'memory'), '');
    }

    // Recent activity (shorter for group — just last 15 lines)
    const activity = this.readTail(this.activityPath, 15);
    if (activity) {
      parts.push('--- RECENT ACTIVITY ---', this.scrubContext(activity, 'activity'), '');
    }

    // Recent channel messages for context
    if (recentChannelHistory.length > 0) {
      const scrubbedHistory = recentChannelHistory.map(m => this.scrubContext(m, 'slack_history'));
      parts.push('--- RECENT CHANNEL MESSAGES ---', scrubbedHistory.join('\n'), '');
    }

    // The trigger message
    parts.push('--- CONVERSATION TOPIC ---', this.scrubContext(triggerMessage, 'trigger_message'), '');

    // Prior turns in this conversation
    if (priorTurns.length > 0) {
      const turnText = priorTurns
        .map(t => `**${t.roleName}**: ${t.text}`)
        .join('\n\n');
      parts.push('--- PRIOR RESPONSES IN THIS CONVERSATION ---', this.scrubContext(turnText, 'prior_turns'), '');
      parts.push(`${priorTurns.length} team member(s) have already responded above. Add your perspective — don't repeat what they said.`);
    } else {
      parts.push('You are the FIRST to respond in this group conversation. Set the tone and frame the discussion from your perspective.');
    }

    // Final defense-in-depth pass on assembled context
    return this.scrubContext(parts.join('\n'), 'assembled_group');
  }

  assemble(role: RoleConfig, recentChannelHistory: string[]): string {
    const parts: string[] = [BRIDGE_PREAMBLE, ''];

    // Role identity (scrub each source independently)
    const claudeMd = this.readFileSafe(role.claudeMdPath);
    if (claudeMd) {
      parts.push('--- ROLE IDENTITY ---', this.scrubContext(claudeMd, 'role_identity'), '');
    }

    // Memory
    const memory = this.readFileSafe(role.memoryPath);
    if (memory) {
      parts.push('--- MEMORY ---', this.scrubContext(memory, 'memory'), '');
    }

    // Briefs inbox (listing only — filenames, no content to scrub)
    const briefsList = this.listBriefs(role.briefsPath);
    if (briefsList) {
      parts.push('--- PENDING BRIEFS ---', briefsList, '');
    }

    // Recent activity
    const activity = this.readTail(this.activityPath, 30);
    if (activity) {
      parts.push('--- RECENT ACTIVITY ---', this.scrubContext(activity, 'activity'), '');
    }

    // Recent channel messages for conversational context
    if (recentChannelHistory.length > 0) {
      const scrubbedHistory = recentChannelHistory.map(m => this.scrubContext(m, 'slack_history'));
      parts.push('--- RECENT CHANNEL MESSAGES ---', scrubbedHistory.join('\n'), '');
    }

    // Final defense-in-depth pass on assembled context
    return this.scrubContext(parts.join('\n'), 'assembled_normal');
  }

  readFileSafe(filePath: string): string | null {
    try {
      if (fs.existsSync(filePath)) {
        return fs.readFileSync(filePath, 'utf-8');
      }
    } catch (err) {
      log('warn', `Could not read ${filePath}: ${err}`);
    }
    return null;
  }

  private listBriefs(briefsPath: string): string | null {
    try {
      if (!fs.existsSync(briefsPath)) return null;
      const files = fs.readdirSync(briefsPath)
        .filter(f => f.endsWith('.md'))
        .sort()
        .reverse()
        .slice(0, 10);
      if (files.length === 0) return null;
      return files.map(f => {
        const stat = fs.statSync(path.join(briefsPath, f));
        return `- ${f} (${stat.mtime.toISOString().split('T')[0]})`;
      }).join('\n');
    } catch (err) {
      log('warn', `Could not list briefs at ${briefsPath}: ${err}`);
      return null;
    }
  }

  private readTail(filePath: string, lines: number): string | null {
    try {
      if (!fs.existsSync(filePath)) return null;
      const content = fs.readFileSync(filePath, 'utf-8');
      const allLines = content.split('\n');
      return allLines.slice(-lines).join('\n');
    } catch (err) {
      log('warn', `Could not read tail of ${filePath}: ${err}`);
      return null;
    }
  }
}
