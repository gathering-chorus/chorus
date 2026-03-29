import { WebClient } from '@slack/web-api';
import { loadConfig, RoleConfig } from './config';
import { ChannelMonitor, SlackMessage } from './channel-monitor';
import { MessageRouter, RoutedMessage } from './router';
import { ContextAssembler } from './context-assembler';
import { ClaudeClient } from './claude-client';
import { ResponsePoster } from './response-poster';
import { RateLimiter } from './rate-limiter';
import { GroupConversationOrchestrator } from './group-conversation';
import { ConversationEventLogger } from './conversation-event-logger';
import { startMetricsServer, metrics } from './metrics';
import { handleCommand } from './commands';
import { BriefWatcher } from './brief-watcher';
import { initPodBriefClient } from './pod-brief-io';
import { log, setLogLevel } from './logger';

async function main(): Promise<void> {
  const config = loadConfig();
  setLogLevel(config.logLevel as 'debug' | 'info' | 'warn' | 'error');

  // Initialize pod brief client (filesystem-only if env vars missing)
  initPodBriefClient();

  log('info', 'Starting Slack-to-Claude bridge', {
    roles: config.roles.roles.map(r => r.name),
    pollInterval: config.roles.pollIntervalMs,
    groupConversation: config.roles.groupConversation?.enabled ?? false,
  });

  // Initialize components
  const slack = new WebClient(config.slackBotToken);
  const monitor = new ChannelMonitor(slack);
  const router = new MessageRouter(config.roles);
  const assembler = new ContextAssembler();
  const claude = new ClaudeClient(config.anthropicApiKey);
  const poster = new ResponsePoster(slack);
  const limiter = new RateLimiter(
    config.roles.roles[0]?.maxCallsPerHour || 15,
    config.roles.globalMaxCallsPerHour,
  );

  // Group conversation components
  const eventLogger = new ConversationEventLogger();
  let groupOrchestrator: GroupConversationOrchestrator | null = null;
  if (config.roles.groupConversation?.enabled) {
    groupOrchestrator = new GroupConversationOrchestrator(
      monitor,
      assembler,
      claude,
      poster,
      limiter,
      eventLogger,
      config.roles.groupConversation,
    );
    log('info', 'Group conversation enabled', {
      triggers: config.roles.groupConversation.triggers,
      defaultOrder: config.roles.groupConversation.defaultTurnOrder,
    });
  }

  // Brief watcher — auto-notify roles when new briefs arrive
  const briefWatcher = new BriefWatcher(
    config.roles.roles,
    monitor,
    poster,
    '/app/data',
  );

  // Start metrics server
  startMetricsServer(config.port);

  // Initialize channel monitor (resolves channel names -> IDs)
  await monitor.initialize();

  // Build list of channels to poll
  const channelNames = [
    ...config.roles.roles.map(r => r.channel),
    config.roles.sharedChannel,
  ];

  log('info', `Polling channels: ${channelNames.join(', ')}`);

  // Main poll loop — self-scheduling to avoid overlapping during long group conversations
  let running = true;

  async function pollCycle(): Promise<void> {
    try {
      const messages = await monitor.poll(channelNames);

      for (const msg of messages) {
        // Check for bridge commands (handled without Claude calls)
        if (await handleCommand(msg, poster, config.roles.roles)) continue;

        // Use enhanced routing if group conversations are enabled
        if (groupOrchestrator) {
          const result = router.routeWithMode(msg);
          if (!result) continue;

          if (result.mode === 'group') {
            // Skip if a group conversation is already in progress
            if (groupOrchestrator.isActive) {
              log('info', 'Group conversation already active, queuing as normal');
            } else {
              await groupOrchestrator.run(
                result.triggerMessage,
                result.roles,
                msg.channelName,
              );
              continue;
            }
          }

          // Normal mode — process each role individually
          for (const role of result.roles) {
            await processNormalMessage(msg, role);
          }
        } else {
          // Fallback: group conversations disabled — use original routing
          const routed: RoutedMessage[] = router.route(msg);
          for (const { message, role } of routed) {
            await processNormalMessage(message, role);
          }
        }
      }
      // Scan for new briefs each cycle
      await briefWatcher.scan();
    } catch (err) {
      log('error', `Poll cycle error: ${err}`);
      metrics.errors.inc({ type: 'poll_cycle' });
    }

    // Self-schedule next poll
    if (running) {
      setTimeout(pollCycle, config.roles.pollIntervalMs);
    }
  }

  async function processNormalMessage(
    message: SlackMessage,
    role: RoleConfig
  ): Promise<void> {
    metrics.messagesReceived.inc({ channel: message.channelName, role: role.name });

    // Rate limit check
    const check = limiter.canProceed(role.name, message.channel);
    if (!check.allowed) {
      if (check.notifyRateLimit) {
        await poster.postRateLimited(message.channel, role.name, message.thread_ts);
      }
      log('info', `Rate limited: ${role.name} (${check.reason})`);
      return;
    }

    // Build context
    const recentHistory = await getRecentHistory(slack, message.channel, 10);
    const systemPrompt = assembler.assemble(role, recentHistory);

    // Get Claude response
    try {
      const response = await claude.respond(systemPrompt, message.text, role.name);
      await poster.post(message.channel, role.name, response, message.thread_ts);
      limiter.record(role.name, message.channel);
    } catch (err) {
      log('error', `Failed to respond as ${role.name}: ${err}`);
      metrics.errors.inc({ type: 'response_flow' });
    }
  }

  // Start polling
  await pollCycle();

  log('info', 'Bridge is running');

  // Graceful shutdown
  function shutdown(signal: string): void {
    log('info', `${signal} received, shutting down`);
    running = false;
    process.exit(0);
  }

  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
}

async function getRecentHistory(slack: WebClient, channelId: string, count: number): Promise<string[]> {
  try {
    const result = await slack.conversations.history({ channel: channelId, limit: count });
    return (result.messages || [])
      .reverse()
      .filter(m => m.text)
      .map(m => m.text as string);
  } catch {
    return [];
  }
}

main().catch(err => {
  log('error', `Fatal error: ${err}`);
  process.exit(1);
});
