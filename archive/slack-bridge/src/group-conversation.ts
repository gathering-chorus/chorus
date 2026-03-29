import { RoleConfig, GroupConversationConfig } from './config';
import { ChannelMonitor, SlackMessage } from './channel-monitor';
import { ContextAssembler, ConversationTurn } from './context-assembler';
import { ClaudeClient, ClaudeResponse } from './claude-client';
import { ResponsePoster, truncateForGroup } from './response-poster';
import { RateLimiter } from './rate-limiter';
import { ConversationEventLogger } from './conversation-event-logger';
import { writeCommitmentBriefs } from './commitment-brief-writer';
import { log } from './logger';
import { metrics } from './metrics';

export class GroupConversationOrchestrator {
  private monitor: ChannelMonitor;
  private assembler: ContextAssembler;
  private claude: ClaudeClient;
  private poster: ResponsePoster;
  private limiter: RateLimiter;
  private eventLogger: ConversationEventLogger;
  private config: GroupConversationConfig;
  private _active = false;

  constructor(
    monitor: ChannelMonitor,
    assembler: ContextAssembler,
    claude: ClaudeClient,
    poster: ResponsePoster,
    limiter: RateLimiter,
    eventLogger: ConversationEventLogger,
    config: GroupConversationConfig
  ) {
    this.monitor = monitor;
    this.assembler = assembler;
    this.claude = claude;
    this.poster = poster;
    this.limiter = limiter;
    this.eventLogger = eventLogger;
    this.config = config;
  }

  get isActive(): boolean {
    return this._active;
  }

  /**
   * Run a group conversation. Executes roles sequentially,
   * passing prior turns to each subsequent role.
   */
  async run(
    triggerMessage: SlackMessage,
    roles: RoleConfig[],
    channelName: string
  ): Promise<void> {
    if (this._active) {
      log('warn', 'Group conversation already active, skipping');
      return;
    }

    this._active = true;
    const startTime = Date.now();
    const conversationId = this.eventLogger.generateConversationId();
    const roleNames = roles.map(r => r.name);

    log('info', `Starting group conversation ${conversationId}`, {
      channel: channelName,
      roles: roleNames,
      trigger: triggerMessage.text.slice(0, 100),
    });

    // Check rate limits for all roles before starting
    const rateCheck = this.limiter.canProceedGroup(roleNames);
    if (!rateCheck.allowed) {
      log('warn', `Group conversation blocked by rate limit: ${rateCheck.reason}`, {
        blockedRole: rateCheck.blockedRole,
      });
      await this.poster.postRateLimited(
        triggerMessage.channel,
        rateCheck.blockedRole || 'team',
        triggerMessage.thread_ts
      );
      this._active = false;
      return;
    }

    // Log conversation start
    this.eventLogger.logStart(
      conversationId,
      channelName,
      triggerMessage.text,
      roleNames,
      roleNames
    );

    const priorTurns: ConversationTurn[] = [];
    let totalInputTokens = 0;
    let totalOutputTokens = 0;
    let completedTurns = 0;

    for (let i = 0; i < roles.length; i++) {
      const role = roles[i];

      try {
        // Get recent channel messages excluding this role's own bridge output
        const recentMessages = await this.monitor.getRecentMessagesForGroup(
          channelName,
          role.name,
          15
        );
        const recentHistory = recentMessages.map(m => m.text);

        // Assemble group context with prior turns
        const systemPrompt = this.assembler.assembleForGroup(
          role,
          priorTurns,
          triggerMessage.text,
          recentHistory
        );

        // Call Claude for this role (hard token cap from config)
        const response: ClaudeResponse = await this.claude.respond(
          systemPrompt,
          triggerMessage.text,
          role.name,
          true,
          this.config.maxTokensPerTurn
        );

        // Strip markdown formatting — group responses should read like speech, not briefs
        // Then hard-cap word count — model often ignores prompt constraints
        const cleanText = truncateForGroup(stripGroupFormatting(response.text));

        // Post the response to Slack
        await this.poster.post(
          triggerMessage.channel,
          role.name,
          cleanText,
          triggerMessage.thread_ts
        );

        // Record rate limit usage (no channel debounce for group turns)
        this.limiter.recordGroupTurn(role.name);

        // Track this turn for subsequent roles
        priorTurns.push({ roleName: role.name, text: response.text });
        totalInputTokens += response.inputTokens;
        totalOutputTokens += response.outputTokens;
        completedTurns++;

        // Log the turn event
        this.eventLogger.logTurn(
          conversationId,
          channelName,
          role.name,
          i + 1,
          response.inputTokens,
          response.outputTokens
        );

        metrics.responsesSent.inc({ channel: channelName, role: role.name });

        // Check token budget — only count OUTPUT tokens (what roles said, not context assembly)
        if (this.config.maxTokensPerConversation && totalOutputTokens >= this.config.maxTokensPerConversation) {
          log('info', `Output token budget reached (${totalOutputTokens}/${this.config.maxTokensPerConversation}), ending conversation after ${role.name}`);
          break;
        }

        // Delay between turns to let Slack propagate
        if (i < roles.length - 1) {
          await sleep(this.config.turnDelayMs);

          // Check if Jeff posted something — if so, stop the sequence
          const humanMsg = await this.monitor.checkForHumanMessage(
            channelName,
            triggerMessage.ts
          );
          if (humanMsg) {
            log('info', `Human intervened mid-conversation, stopping after ${role.name}`, {
              humanMessage: humanMsg.text.slice(0, 100),
            });
            break;
          }
        }
      } catch (err) {
        log('error', `Group conversation turn failed for ${role.name}: ${err}`);
        metrics.errors.inc({ type: 'group_conversation_turn' });
        // Continue with remaining roles — partial conversation is better than none
      }
    }

    // Log conversation end
    const durationMs = Date.now() - startTime;
    this.eventLogger.logEnd(
      conversationId,
      channelName,
      completedTurns,
      totalInputTokens,
      totalOutputTokens,
      durationMs
    );

    log('info', `Group conversation ${conversationId} complete`, {
      turns: completedTurns,
      durationMs,
      totalInputTokens,
      totalOutputTokens,
    });

    // Write commitment briefs for any role that said "I'll do X"
    if (priorTurns.length > 0) {
      try {
        await writeCommitmentBriefs(
          triggerMessage.text,
          channelName,
          priorTurns,
          roles
        );
      } catch (err) {
        log('error', `Failed to write commitment briefs: ${err}`);
      }
    }

    // Post cost summary to channel — one-liner so Jeff sees the spend
    const totalTokens = totalInputTokens + totalOutputTokens;
    const estimatedCost = estimateConversationCost(totalInputTokens, totalOutputTokens);
    const durationSec = Math.round(durationMs / 1000);
    const summaryText = `_${completedTurns} turns, ${totalTokens.toLocaleString()} tokens, ~$${estimatedCost.toFixed(3)}, ${durationSec}s_`;
    try {
      await this.poster.postSystem(
        triggerMessage.channel,
        summaryText,
        triggerMessage.thread_ts
      );
    } catch (err) {
      log('warn', `Failed to post cost summary: ${err}`);
    }

    this._active = false;
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Rough cost estimate for a conversation using Sonnet pricing.
 * Input: $3/M tokens, Output: $15/M tokens (Sonnet 4)
 */
function estimateConversationCost(inputTokens: number, outputTokens: number): number {
  return (inputTokens * 3 + outputTokens * 15) / 1_000_000;
}

/**
 * Strip markdown formatting that makes group responses look like briefs instead of conversation.
 * Removes: headers (#), bold section markers (**Title:**), bullet lists (- or *), horizontal rules (---)
 */
function stripGroupFormatting(text: string): string {
  return text
    .split('\n')
    .map(line => {
      // Strip markdown headers (# ## ### etc)
      line = line.replace(/^#{1,4}\s+/, '');
      // Strip horizontal rules
      if (/^-{3,}$/.test(line.trim())) return '';
      // Strip bullet points but keep the text
      line = line.replace(/^[\s]*[-*]\s+/, '');
      // Strip numbered lists but keep the text
      line = line.replace(/^[\s]*\d+\.\s+/, '');
      return line;
    })
    .filter(line => line.trim() !== '')
    .join('\n')
    // Collapse multiple newlines
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}
