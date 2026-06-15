/* eslint-disable security/detect-non-literal-fs-filename --
 * fs paths from server-controlled SCAN_DIR/PULSE_FILE env constants; reads on
 * `${SCAN_DIR}/${role}-declared.json` where role is a member of the 4-element
 * ROLES tuple. Object indexing keyed by validated role names from the same tuple.
 */
import fs from 'fs';
import path from 'path';

// #2167: env-configurable so tests can point at a fixture directory.
const SCAN_DIR = process.env.CLEARING_SCAN_DIR || '/tmp/claude-team-scan';
const PULSE_FILE = process.env.CLEARING_PULSE_FILE || '/tmp/pulse-latest.json';
const CHORUS_API = process.env.CHORUS_API_BASE || 'http://localhost:3340';
const ROLES = ['jeff', 'wren', 'silas', 'kade'] as const;

interface BoardCard { id: number; owner?: string; status?: string; title?: string; domain?: string; }

export interface RoleTile {
  role: string;
  state: string;
  card: string;
  lastAction: string;
  lastActionAge: string;
  sessionAlive: boolean;
  /** #2168 — ALL WIP cards owned by this role, sourced from pulse.board.wip_cards.
   *  Tile must surface every card the role owns, not just their declared one. */
  cards?: string[];
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

export interface TilePollerOptions {
  scanDir?: string;
  pulseFile?: string;
  chorusApi?: string;
}

export class TilePoller {
  private tiles: Map<string, RoleTile> = new Map();
  private pulse: PulseState | null = null;
  private boardCache: { wip_cards: BoardCard[]; swat_cards: BoardCard[]; ts: number } = { wip_cards: [], swat_cards: [], ts: 0 };
  // #2273: exposed so tests can await the board refresh instead of using setTimeout
  boardRefresh: Promise<void> = Promise.resolve();
  private readonly scanDir: string;
  private readonly pulseFile: string;
  private readonly chorusApi: string;

  constructor(opts: TilePollerOptions = {}) {
    this.scanDir = opts.scanDir ?? SCAN_DIR;
    this.pulseFile = opts.pulseFile ?? PULSE_FILE;
    this.chorusApi = opts.chorusApi ?? CHORUS_API;
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
    this.refreshBoardFromApi();
  }

  poll(): void {
    for (const role of ROLES) {
      const tile = this.readRoleTile(role);
      this.tiles.set(role, tile);
    }
    this.pulse = this.readPulse();
    this.refreshBoardFromApi();
  }

  private refreshBoardFromApi(): void {
    this.boardRefresh = Promise.allSettled([
      fetch(`${this.chorusApi}/api/chorus/context/board/wip`).then((r) => r.ok ? r.json() : null).catch(() => null),
      fetch(`${this.chorusApi}/api/chorus/context/board/swat`).then((r) => r.ok ? r.json() : null).catch(() => null),
    ]).then(([wipResult, swatResult]) => {
      const wipData = wipResult.status === 'fulfilled' ? wipResult.value : null;
      const swatData = swatResult.status === 'fulfilled' ? swatResult.value : null;
      const wip: BoardCard[] = (wipData?.data?.cards ?? []).map((c: BoardCard) => ({ ...c, status: 'WIP' }));
      const swat: BoardCard[] = (swatData?.data?.cards ?? []).map((c: BoardCard) => ({ ...c, status: 'SWAT' }));
      this.boardCache = { wip_cards: wip, swat_cards: swat, ts: Date.now() };
    });
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
      const content = fs.readFileSync(this.pulseFile, 'utf-8');
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

  // #2467: clearCard() retired — card is no longer in role-state. Tile
  // renderer derives WIP cards directly from the board (via boardCache).
  // No state mutation needed on card.accepted; the board is authoritative.

  private applyAndonState(tile: RoleTile, role: string): void {
    try {
      const data = JSON.parse(fs.readFileSync(path.join(this.scanDir, `${role}-declared.json`), 'utf-8'));
      tile.state = data.state || 'idle';
      // #2467: card field no longer in role-state; tile.card derived from
      // board in applyBoardAndPulse below.
      tile.sessionAlive = data.session_alive !== false;
      if (data.ts) {
        tile.lastActionAge = formatAge(Math.floor(Date.now() / 1000) - data.ts);
      }
    } catch {
      // File doesn't exist or is malformed — tile keeps defaults.
    }
  }

  // #2168 + #2467 — surface WIP cards owned by this role from the board.
  // Board is the single source of truth for "what cards are this role on";
  // role-state.card field is retired (#2467).
  private applyBoardAndPulse(tile: RoleTile, role: string): void {
    try {
      const ownedWip = this.boardCache.wip_cards
        .filter((c) => (c.owner || '').toLowerCase() === role.toLowerCase())
        .map((c) => `#${c.id}`);
      const ownedSwat = this.boardCache.swat_cards
        .filter((c) => (c.owner || '').toLowerCase() === role.toLowerCase())
        .map((c) => `#${c.id}[swat]`);
      const ownedIds = [...ownedWip, ...ownedSwat];
      if (ownedIds.length > 0) {
        tile.cards = ownedIds;
        // Primary "card" display = first WIP card from the board (board is
        // authoritative; previous shadow-read from role-state retired #2467).
        tile.card = ownedWip[0] ?? ownedIds[0];
      } else {
        tile.card = '';
      }
      // #2467: divergence (cardDeclared vs cardInferred) is moot now —
      // there's no declared-card field to diverge from. Pulse-side divergence
      // flag may eventually retire too; for now we ignore it.
      tile.divergent = false;
    } catch {
      // Board cache absent — tile renders with no cards.
    }
  }

  private applyLastObservation(tile: RoleTile, role: string): void {
    try {
      const lines = fs.readFileSync(path.join(this.scanDir, `${role}-observations.jsonl`), 'utf-8')
        .trim().split('\n').filter(Boolean);
      if (lines.length === 0) return;
      const last = JSON.parse(lines[lines.length - 1]);
      tile.lastAction = last.digest || '';
      if (last.ts) {
        const ageSecs = Math.floor((Date.now() - new Date(last.ts).getTime()) / 1000);
        tile.lastActionAge = formatAge(ageSecs);
      }
    } catch {
      // No observations yet
    }
  }

  private readRoleTile(role: string): RoleTile {
    if (role === 'jeff') return this.readJeffTile();
    const tile: RoleTile = {
      role,
      state: 'idle',
      card: '',
      lastAction: '',
      lastActionAge: '',
      sessionAlive: false,
    };
    this.applyAndonState(tile, role);
    this.applyBoardAndPulse(tile, role);
    this.applyLastObservation(tile, role);
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

    const stateFile = path.join(this.scanDir, 'jeff-input.json');
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
