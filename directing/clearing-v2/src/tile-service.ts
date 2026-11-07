// @domain: messages

import { RoleTile } from './types';

export const DEFAULT_ROLES = ['jeff', 'wren', 'silas', 'kade', 'abby-normal', 'borg-queen'];

export class TileService {
  private pulseUrl: string;
  private tiles: Map<string, RoleTile> = new Map();

  constructor(pulseUrl = 'http://127.0.0.1:3475', roles: string[] = DEFAULT_ROLES) {
    this.pulseUrl = pulseUrl;
    for (const r of roles) {
      this.tiles.set(r, {
        role: r,
        name: r === 'abby-normal' ? 'Abby Normal' : (r === 'borg-queen' ? 'Borg Queen' : r.charAt(0).toUpperCase() + r.slice(1)),
        state: 'idle',
        card: null,
        lastActivity: null,
        lastEvent: null,
        lastSeenAt: null,
        detail: null,
      });
    }
  }

  getTiles(): RoleTile[] {
    return Array.from(this.tiles.values());
  }

  updateTile(role: string, partial: Partial<RoleTile>): void {
    const existing = this.tiles.get(role);
    if (existing) {
      this.tiles.set(role, { ...existing, ...partial });
    }
  }

  /**
   * Refreshes role tiles from Pulse / role-state.
   */
  async refresh(): Promise<RoleTile[]> {
    try {
      // In Chorus, role-state CLI or pulse exposes state
      // Here we provide non-blocking best-effort fetch
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), 2000);

      const res = await fetch(`${this.pulseUrl}/health`, { signal: controller.signal }).catch(() => null);
      clearTimeout(timeout);

      if (res && res.ok) {
        // Pulse is alive; update tiles
        // Role state can be updated incrementally as events arrive
      }
    } catch {
      // Retain last known state on failure — never blank the screen (DEC-3869)
    }

    return this.getTiles();
  }
}
