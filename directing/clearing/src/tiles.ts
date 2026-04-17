import fs from 'fs';
import path from 'path';

const SCAN_DIR = '/tmp/claude-team-scan';
const ROLES = ['jeff', 'wren', 'silas', 'kade'] as const;

export interface RoleTile {
  role: string;
  state: string;
  card: string;
  lastAction: string;
  lastActionAge: string;
  sessionAlive: boolean;
  /** #2120 — reconciler-written card (from observations) when it diverges from declared */
  cardInferred?: string;
  /** #2120 — card the role last manually declared */
  cardDeclared?: string;
  /** #2120 — true when inferred differs from declared and reconciler flipped it */
  divergent?: boolean;
}

export interface PulseState {
  alertsToday: number;
  indexFreshness: { fresh: number; warn: number; critical: number; dead: number };
  nudges: Record<string, { pending: number; stale: boolean }>;
  eventsLast60s: number;
  elapsed_ms: number;
}

export class TilePoller {
  private tiles: Map<string, RoleTile> = new Map();
  private pulse: PulseState | null = null;

  constructor() {
    for (const role of ROLES) {
      this.tiles.set(role, {
        role,
        state: 'unknown',
        card: '',
        lastAction: '',
        lastActionAge: '',
        sessionAlive: false,
      });
    }
    this.poll();
  }

  poll(): void {
    for (const role of ROLES) {
      const tile = this.readRoleTile(role);
      this.tiles.set(role, tile);
    }
    this.pulse = this.readPulse();
  }

  getTiles(): RoleTile[] {
    return ROLES.map((r) => this.tiles.get(r)!);
  }

  getPulse(): PulseState | null {
    return this.pulse;
  }

  /** Read Pulse snapshot (#1881) */
  private readPulse(): PulseState | null {
    try {
      const content = fs.readFileSync('/tmp/pulse-latest.json', 'utf-8');
      const data = JSON.parse(content);
      return {
        alertsToday: data.alerts?.count || 0,
        indexFreshness: data.index_freshness || { fresh: 0, warn: 0, critical: 0, dead: 0 },
        nudges: data.nudges || {},
        eventsLast60s: data.events?.last_60s_count || 0,
        elapsed_ms: data.elapsed_ms || 0,
      };
    } catch {
      return null;
    }
  }

  /** Clear card from role state on acceptance (#2286) */
  clearCard(role: string): void {
    const stateFile = path.join(SCAN_DIR, `${role}-declared.json`);
    try {
      const content = fs.readFileSync(stateFile, 'utf-8');
      const data = JSON.parse(content);
      data.state = 'idle';
      delete data.card;
      delete data.card_type;
      data.ts = Math.floor(Date.now() / 1000);
      fs.writeFileSync(stateFile, JSON.stringify(data));
    } catch {
      // State file doesn't exist — nothing to clear
    }
  }

  private readRoleTile(role: string): RoleTile {
    const tile: RoleTile = {
      role,
      state: 'idle',
      card: '',
      lastAction: '',
      lastActionAge: '',
      sessionAlive: false,
    };

    // Jeff has a different state file format
    if (role === 'jeff') {
      return this.readJeffTile();
    }

    // Read andon state
    const stateFile = path.join(SCAN_DIR, `${role}-declared.json`);
    try {
      const content = fs.readFileSync(stateFile, 'utf-8');
      const data = JSON.parse(content);
      tile.state = data.state || 'idle';
      tile.card = data.card ? `#${data.card}` : '';
      tile.sessionAlive = data.session_alive !== false;

      // #2120 — surface divergence when reconciler has flipped the card
      if (data.source === 'reconciler' && data.card_declared) {
        tile.cardDeclared = String(data.card_declared);
        tile.cardInferred = String(data.card_inferred || data.card);
        tile.divergent = tile.cardDeclared !== tile.cardInferred;
      }

      if (data.ts) {
        const ageSecs = Math.floor(Date.now() / 1000) - data.ts;
        tile.lastActionAge = formatAge(ageSecs);
      }
    } catch {
      // File doesn't exist or is malformed
    }

    // Read last observation for action summary
    const obsFile = path.join(SCAN_DIR, `${role}-observations.jsonl`);
    try {
      const content = fs.readFileSync(obsFile, 'utf-8');
      const lines = content.trim().split('\n').filter(Boolean);
      if (lines.length > 0) {
        const last = JSON.parse(lines[lines.length - 1]);
        tile.lastAction = last.digest || '';

        if (last.ts) {
          const obsTime = new Date(last.ts).getTime();
          const ageSecs = Math.floor((Date.now() - obsTime) / 1000);
          tile.lastActionAge = formatAge(ageSecs);
        }
      }
    } catch {
      // No observations yet
    }

    return tile;
  }

  private readJeffTile(): RoleTile {
    const tile: RoleTile = {
      role: 'jeff',
      state: 'offline',
      card: '',
      lastAction: '',
      lastActionAge: '',
      sessionAlive: false,
    };

    const stateFile = path.join(SCAN_DIR, 'jeff-input.json');
    try {
      const content = fs.readFileSync(stateFile, 'utf-8');
      const data = JSON.parse(content);

      // Derive presence from input metrics
      const updated = data.updated || 0;
      const nowSecs = Math.floor(Date.now() / 1000);
      const ageSecs = nowSecs - updated;

      // Active if input within last 5 minutes
      if (ageSecs < 300) {
        const keysPerMin = data.keys_per_min || 0;
        const clicksPerMin = data.clicks_per_min || 0;
        if (keysPerMin > 0) tile.state = 'directing';
        else if (clicksPerMin > 0 || data.mouse_active) tile.state = 'watching';
        else tile.state = 'present';
        tile.sessionAlive = true;
      } else {
        tile.state = 'away';
        tile.sessionAlive = false;
      }

      // Show input activity as last action
      const kpm = Math.round(data.keys_per_min || 0);
      const cpm = Math.round(data.clicks_per_min || 0);
      if (kpm > 0 || cpm > 0) {
        tile.lastAction = `${kpm} keys/min · ${cpm} clicks/min`;
      }

      tile.lastActionAge = formatAge(ageSecs);
    } catch {
      // No jeff state file
    }

    return tile;
  }
}

function formatAge(secs: number): string {
  if (secs < 0) return 'just now';
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}
