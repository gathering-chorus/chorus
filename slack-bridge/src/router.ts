import { SlackMessage } from './channel-monitor';
import { RoleConfig, RolesConfig, GroupConversationConfig } from './config';

export interface RoutedMessage {
  message: SlackMessage;
  role: RoleConfig;
}

export type RouteMode = 'normal' | 'group';

export interface RoutedResult {
  mode: RouteMode;
  roles: RoleConfig[];
  triggerMessage: SlackMessage;
  /** If a role initiated the group conversation (e.g., @team:wren), this is the initiator name */
  initiator?: string;
}

export class MessageRouter {
  private rolesByChannel: Map<string, RoleConfig> = new Map();
  private rolesByName: Map<string, RoleConfig> = new Map();
  private allRoles: RoleConfig[];
  private sharedChannel: string;
  private groupConfig?: GroupConversationConfig;

  constructor(config: RolesConfig) {
    this.sharedChannel = config.sharedChannel;
    this.allRoles = config.roles;
    this.groupConfig = config.groupConversation;
    for (const role of config.roles) {
      this.rolesByChannel.set(role.channel, role);
      this.rolesByName.set(role.name.toLowerCase(), role);
    }
  }

  /** Original routing — backward compatible, unchanged */
  route(message: SlackMessage): RoutedMessage[] {
    const results: RoutedMessage[] = [];

    // Direct channel — route to that role
    const directRole = this.rolesByChannel.get(message.channelName);
    if (directRole) {
      results.push({ message, role: directRole });
      return results;
    }

    // Shared channel — check for role name mentions
    if (message.channelName === this.sharedChannel) {
      const textLower = message.text.toLowerCase();
      for (const [name, role] of this.rolesByName) {
        if (textLower.includes(name)) {
          results.push({ message, role });
        }
      }
    }

    return results;
  }

  /** Enhanced routing that detects group conversation triggers */
  routeWithMode(message: SlackMessage): RoutedResult | null {
    // Only detect group triggers on the shared channel
    if (message.channelName !== this.sharedChannel) {
      const routed = this.route(message);
      if (routed.length === 0) return null;
      return {
        mode: 'normal',
        roles: routed.map(r => r.role),
        triggerMessage: message,
      };
    }

    // Check for group triggers if enabled
    if (this.groupConfig?.enabled) {
      const textLower = message.text.toLowerCase();

      // Check for role-initiated trigger: @team:wren, @team:silas, @team:kade
      const roleInitiatedMatch = textLower.match(/@team:(\w+)/);
      let initiator: string | undefined;

      if (roleInitiatedMatch) {
        initiator = roleInitiatedMatch[1];
        // Exclude the initiator from respondents — their trigger message IS their turn
        const turnOrder = this.groupConfig.defaultTurnOrder.filter(
          name => name.toLowerCase() !== initiator
        );
        const orderedRoles = this.resolveGroupRoles(turnOrder);
        return {
          mode: 'group',
          roles: orderedRoles,
          triggerMessage: message,
          initiator,
        };
      }

      // Check explicit triggers (e.g., @team)
      const hasGroupTrigger = this.groupConfig.triggers.some(t =>
        textLower.includes(t.toLowerCase())
      );

      // Check if 2+ roles are mentioned
      const mentionedRoles: RoleConfig[] = [];
      for (const [name, role] of this.rolesByName) {
        if (textLower.includes(name)) {
          mentionedRoles.push(role);
        }
      }
      const hasMultiRoleMention = mentionedRoles.length >= 2;

      if (hasGroupTrigger || hasMultiRoleMention) {
        // Resolve turn order based on the configured default
        const orderedRoles = this.resolveGroupRoles(this.groupConfig.defaultTurnOrder);
        return {
          mode: 'group',
          roles: orderedRoles,
          triggerMessage: message,
        };
      }
    }

    // Fall back to normal routing
    const routed = this.route(message);
    if (routed.length === 0) return null;
    return {
      mode: 'normal',
      roles: routed.map(r => r.role),
      triggerMessage: message,
    };
  }

  /** Resolve an ordered list of role names to RoleConfig objects */
  resolveGroupRoles(turnOrder: string[]): RoleConfig[] {
    const roles: RoleConfig[] = [];
    for (const name of turnOrder) {
      const role = this.rolesByName.get(name.toLowerCase());
      if (role) roles.push(role);
    }
    // If resolution failed, fall back to all roles in config order
    return roles.length > 0 ? roles : this.allRoles;
  }

  /** Get a named turn order from config, or fall back to default */
  getTurnOrder(modeName?: string): string[] {
    if (!this.groupConfig) return this.allRoles.map(r => r.name);
    if (modeName && this.groupConfig.turnOrders[modeName]) {
      return this.groupConfig.turnOrders[modeName];
    }
    return this.groupConfig.defaultTurnOrder;
  }
}
