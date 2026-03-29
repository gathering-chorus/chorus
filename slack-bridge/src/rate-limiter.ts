import { log } from './logger';
import { metrics } from './metrics';

interface RoleBucket {
  count: number;
  windowStart: number;
  rateLimitNotified: boolean;
}

interface ChannelDebounce {
  lastResponse: number;
}

export class RateLimiter {
  private perRole: Map<string, RoleBucket> = new Map();
  private perChannel: Map<string, ChannelDebounce> = new Map();
  private globalCount = 0;
  private globalWindowStart = Date.now();

  private roleMaxPerHour: number;
  private globalMaxPerHour: number;
  private debounceMs: number;

  constructor(roleMaxPerHour = 15, globalMaxPerHour = 30, debounceMs = 10000) {
    this.roleMaxPerHour = roleMaxPerHour;
    this.globalMaxPerHour = globalMaxPerHour;
    this.debounceMs = debounceMs;
  }

  canProceed(roleName: string, channelId: string): { allowed: boolean; reason?: string; notifyRateLimit: boolean } {
    const now = Date.now();

    // Check channel debounce
    const channelState = this.perChannel.get(channelId);
    if (channelState && now - channelState.lastResponse < this.debounceMs) {
      return { allowed: false, reason: 'channel_debounce', notifyRateLimit: false };
    }

    // Reset hourly windows if needed
    this.resetWindowIfNeeded(now);

    // Check global limit
    if (this.globalCount >= this.globalMaxPerHour) {
      log('warn', 'Global rate limit reached');
      metrics.rateLimited.inc({ role: 'global' });
      return { allowed: false, reason: 'global_limit', notifyRateLimit: false };
    }

    // Check per-role limit
    const roleBucket = this.getOrCreateRoleBucket(roleName, now);
    if (roleBucket.count >= this.roleMaxPerHour) {
      const shouldNotify = !roleBucket.rateLimitNotified;
      if (shouldNotify) {
        roleBucket.rateLimitNotified = true;
      }
      metrics.rateLimited.inc({ role: roleName });
      return { allowed: false, reason: 'role_limit', notifyRateLimit: shouldNotify };
    }

    return { allowed: true, notifyRateLimit: false };
  }

  /**
   * Check if all roles in a group conversation can proceed.
   * Skips channel debounce (group turns post rapidly in sequence).
   * Returns the first role that can't proceed, or null if all can.
   */
  canProceedGroup(roleNames: string[]): { allowed: boolean; blockedRole?: string; reason?: string } {
    const now = Date.now();
    this.resetWindowIfNeeded(now);

    // Check global limit has room for all roles
    if (this.globalCount + roleNames.length > this.globalMaxPerHour) {
      return { allowed: false, reason: 'global_limit' };
    }

    // Check each role has capacity
    for (const roleName of roleNames) {
      const bucket = this.getOrCreateRoleBucket(roleName, now);
      if (bucket.count >= this.roleMaxPerHour) {
        return { allowed: false, blockedRole: roleName, reason: 'role_limit' };
      }
    }

    return { allowed: true };
  }

  /** Record a group conversation turn (no channel debounce set) */
  recordGroupTurn(roleName: string): void {
    this.globalCount++;
    const bucket = this.getOrCreateRoleBucket(roleName, Date.now());
    bucket.count++;
  }

  record(roleName: string, channelId: string): void {
    const now = Date.now();

    // Record global
    this.globalCount++;

    // Record per-role
    const bucket = this.getOrCreateRoleBucket(roleName, now);
    bucket.count++;

    // Record channel debounce
    this.perChannel.set(channelId, { lastResponse: now });
  }

  private getOrCreateRoleBucket(roleName: string, now: number): RoleBucket {
    let bucket = this.perRole.get(roleName);
    if (!bucket) {
      bucket = { count: 0, windowStart: now, rateLimitNotified: false };
      this.perRole.set(roleName, bucket);
    }
    return bucket;
  }

  private resetWindowIfNeeded(now: number): void {
    const oneHour = 60 * 60 * 1000;

    // Reset global
    if (now - this.globalWindowStart >= oneHour) {
      this.globalCount = 0;
      this.globalWindowStart = now;
    }

    // Reset per-role
    for (const [name, bucket] of this.perRole) {
      if (now - bucket.windowStart >= oneHour) {
        this.perRole.set(name, { count: 0, windowStart: now, rateLimitNotified: false });
      }
    }
  }
}
