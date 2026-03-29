import * as fs from 'fs';
import * as path from 'path';

export interface RoleConfig {
  name: string;
  channel: string;
  claudeMdPath: string;
  memoryPath: string;
  briefsPath: string;
  maxCallsPerHour: number;
}

export interface GroupConversationConfig {
  enabled: boolean;
  triggers: string[];
  defaultTurnOrder: string[];
  turnOrders: Record<string, string[]>;
  maxTurnsPerConversation: number;
  turnDelayMs: number;
  maxTokensPerTurn: number;
  maxTokensPerConversation: number;
}

export interface RolesConfig {
  roles: RoleConfig[];
  sharedChannel: string;
  pollIntervalMs: number;
  globalMaxCallsPerHour: number;
  groupConversation?: GroupConversationConfig;
}

export interface BridgeConfig {
  slackBotToken: string;
  anthropicApiKey: string;
  port: number;
  logLevel: string;
  roles: RolesConfig;
}

export function loadConfig(): BridgeConfig {
  const slackBotToken = requireEnv('SLACK_BOT_TOKEN');
  const anthropicApiKey = requireEnv('ANTHROPIC_API_KEY');
  const port = parseInt(process.env.BRIDGE_PORT || '3460', 10);
  const logLevel = process.env.LOG_LEVEL || 'info';

  const rolesPath = path.resolve(__dirname, '../config/roles.json');
  const rolesRaw = fs.readFileSync(rolesPath, 'utf-8');
  const roles: RolesConfig = JSON.parse(rolesRaw);

  validateRolesConfig(roles);

  return { slackBotToken, anthropicApiKey, port, logLevel, roles };
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function validateRolesConfig(config: RolesConfig): void {
  if (!config.roles || config.roles.length === 0) {
    throw new Error('roles.json must define at least one role');
  }
  if (!config.sharedChannel) {
    throw new Error('roles.json must define sharedChannel');
  }
  if (!config.pollIntervalMs || config.pollIntervalMs < 5000) {
    throw new Error('pollIntervalMs must be at least 5000ms');
  }
  for (const role of config.roles) {
    if (!role.name || !role.channel || !role.claudeMdPath) {
      throw new Error(`Role ${role.name || 'unknown'} missing required fields`);
    }
  }
}
