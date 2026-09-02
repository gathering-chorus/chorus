/* eslint-disable security/detect-non-literal-fs-filename --
 * fs paths from server-controlled SCAN_DIR/PULSE_FILE env constants; reads on
 * `${SCAN_DIR}/${role}-declared.json` where role is a member of the 4-element
 * ROLES tuple. Object indexing keyed by validated role names from the same tuple.
 */
import fs from 'fs';
import path from 'path';
import { isRenderableDigest } from './observations';
import { AGENT_ROLES, projectRoleState, type SpineActivity } from './tiles-spine';
import { spinePath, projectSpine } from './spine-tail';

// #2167: env-configurable so tests can point at a fixture directory.
const SCAN_DIR = process.env.CLEARING_SCAN_DIR || '/tmp/claude-team-scan';
const PULSE_FILE = process.env.CLEARING_PULSE_FILE || '/tmp/pulse-latest.json';
const CHORUS_API = process.env.CHORUS_API_BASE || 'http://localhost:3340';
// #3772 — werk run pins; a live pipeline must show on the tile (Jeff's 10:17
// screenshot: three "idle" tiles while two pipelines and a build were running).
const WERK_RUNS_DIR = process.env.CLEARING_WERK_RUNS_DIR
  || path.join(process.env.HOME || '/Users/jeffbridwell', '.chorus', 'werk-runs');
const ROLES = ['jeff', 'wren', 'silas', 'kade'] as const;

interface BoardCard { id: number; owner?: string; status?: string; title?: string; domain?: string; }

/** A werk-run pin as written to disk. Every field optional on purpose: this is
 *  parsed from a file another process wrote, and a missing field must read as
 *  "cannot prove it" rather than crash the poll. */
interface WerkRunPin {
  role?: string;
  card?: number;
  phase?: string;
  pid?: number;
  presentedAt?: string;
  startedAt?: string;
}

/**
 * #3869 — how long after a role's last observed command it still counts as
 * working. Two minutes: long enough to span a build step or a considered
 * reply, short enough that a dead session shows as idle before Jeff has to ask.
 */
export const ACTIVE_WINDOW_SECS = 120;

/**
 * #3869 — the role card's state, derived rather than remembered.
 *
 * `declared` is what the role last wrote to `<role>-declared.json` by hand.
 * It is authoritative ONLY for states no amount of observation can infer:
 * a role saying it is blocked, or that it is observing someone. Those often
 * come with continued command activity — a blocked role investigates — so
 * inference must not overwrite them.
 *
 * Everything else is a fact about whether commands are landing. A declaration
 * of `building` that has gone quiet for an hour is not information; it is the
 * last time somebody remembered to type it.
 */
export function deriveState(input: {
  declared: string;
  lastActivityAgeSecs: number | null;
}): string {
  const active =
    input.lastActivityAgeSecs !== null && input.lastActivityAgeSecs < ACTIVE_WINDOW_SECS;

  // A stale declaration of ANY kind, including blocked, decays to idle. A
  // "blocked" card from three hours ago tells Jeff nothing about now.
  if (!active) return 'idle';

  if (input.declared === 'blocked' || input.declared === 'observing') return input.declared;
  return 'building';
}

export type BoardCacheState = { wip_cards: BoardCard[]; swat_cards: BoardCard[]; ts: number };

/**
 * #3869 — fold a board poll into the cache without letting a failed read blank
 * the screen.
 *
 * A rejected fetch, a non-ok response, and a body missing `data.cards` all
 * arrive here as `null`. The old code turned every one of those into `[]` and
 * replaced the cache with it, so a single bad poll wiped the cards off Jeff's
 * tiles until the next good one — the flicker he described, on the polling
 * interval.
 *
 * `null` means "could not ask". `[]` means "asked, nothing there". They are
 * different facts and only the second one may clear the screen.
 */
export function mergeBoardRefresh(
  prev: BoardCacheState,
  read: { wip: BoardCard[] | null; swat: BoardCard[] | null },
  now: number,
): BoardCacheState {
  const learnedSomething = read.wip !== null || read.swat !== null;
  return {
    wip_cards: read.wip ?? prev.wip_cards,
    swat_cards: read.swat ?? prev.swat_cards,
    // Only advance the stamp when a read actually succeeded, so freshness
    // never claims to know something it could not fetch.
    ts: learnedSomething ? now : prev.ts,
  };
}

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
  werkRunsDir?: string;
  /** #3905 — the spine the projection reads. Tests MUST pin a fixture; the
   *  live default made three 3772 cases red whenever a 03:00 pipeline ran. */
  spineFile?: string;
  /** #4028 — the derived role rows (GET /api/chorus/context/roles). Tests inject
   *  a sync reader; production fills a cache from the API on every refresh. */
  readRoles?: () => DerivedRoleRow[] | null;
}

/** #4028 — one row of /api/chorus/context/roles, the only source of role STATE. */
export interface DerivedRoleRow {
  role: string;
  state: string;
  detail?: string | null;
  lastActivity?: string | null;
  stale?: boolean;
}

export class TilePoller {
  private tiles: Map<string, RoleTile> = new Map();
  private spineActivity: Record<string, SpineActivity> = {};
  private pulse: PulseState | null = null;
  private boardCache: { wip_cards: BoardCard[]; swat_cards: BoardCard[]; ts: number } = { wip_cards: [], swat_cards: [], ts: 0 };
  // #4028 — derived rows from chorus-api; replaces <role>-declared.json.
  private rolesFromApi: Map<string, DerivedRoleRow> = new Map();
  private readRolesOverride: (() => DerivedRoleRow[] | null) | null = null;
  // #2273: exposed so tests can await the board refresh instead of using setTimeout
  boardRefresh: Promise<void> = Promise.resolve();
  private readonly scanDir: string;
  private readonly pulseFile: string;
  private readonly chorusApi: string;
  private readonly werkRunsDir: string;
  private readonly spineFile: string;

  constructor(opts: TilePollerOptions = {}) {
    this.scanDir = opts.scanDir ?? SCAN_DIR;
    this.pulseFile = opts.pulseFile ?? PULSE_FILE;
    this.chorusApi = opts.chorusApi ?? CHORUS_API;
    this.werkRunsDir = opts.werkRunsDir ?? WERK_RUNS_DIR;
    this.readRolesOverride = opts.readRoles ?? null;
    // A fixture scanDir without an explicit spine means a HERMETIC caller —
    // point the spine into the same fixture dir (usually absent → no
    // projection) instead of the live log.
    this.spineFile = opts.spineFile
      ?? process.env.CLEARING_SPINE_FILE
      ?? (opts.scanDir ? path.join(opts.scanDir, 'chorus.log') : spinePath(process.env));
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
    // #3882 — one spine read, one clock, for every tile this poll.
    this.spineActivity = this.readSpineActivity(Date.now());
    for (const role of ROLES) {
      const tile = this.readRoleTile(role);
      this.tiles.set(role, tile);
    }
    this.pulse = this.readPulse();
    this.refreshBoardFromApi();
  }

  private refreshBoardFromApi(): void {
    if (this.readRolesOverride) {
      const rows = this.readRolesOverride();
      if (rows) this.rolesFromApi = new Map(rows.map((r) => [r.role, r]));
    }
    this.boardRefresh = Promise.allSettled([
      fetch(`${this.chorusApi}/api/chorus/context/board/wip`).then((r) => r.ok ? r.json() : null).catch(() => null),
      fetch(`${this.chorusApi}/api/chorus/context/board/swat`).then((r) => r.ok ? r.json() : null).catch(() => null),
      this.readRolesOverride
        ? Promise.resolve(null)
        : fetch(`${this.chorusApi}/api/chorus/context/roles`).then((r) => r.ok ? r.json() : null).catch(() => null),
    ]).then(([wipResult, swatResult, rolesResult]) => {
      // #4028 — role STATE comes from the derived endpoint; a failed poll keeps
      // the last good rows (same hold-last-good rule as the board below).
      const rolesData = rolesResult.status === 'fulfilled' ? rolesResult.value : null;
      const rows: DerivedRoleRow[] | null = rolesData?.data?.roles ?? null;
      if (rows) this.rolesFromApi = new Map(rows.map((r) => [r.role, r]));
      const wipData = wipResult.status === 'fulfilled' ? wipResult.value : null;
      const swatData = swatResult.status === 'fulfilled' ? swatResult.value : null;
      // #3869 — `null` is preserved all the way to the merge. Collapsing it to
      // `[]` here is what made a failed poll indistinguishable from an empty
      // board, and blanked Jeff's cards once per bad interval.
      const wip: BoardCard[] | null = wipData?.data?.cards
        ? wipData.data.cards.map((c: BoardCard) => ({ ...c, status: 'WIP' }))
        : null;
      const swat: BoardCard[] | null = swatData?.data?.cards
        ? swatData.data.cards.map((c: BoardCard) => ({ ...c, status: 'SWAT' }))
        : null;
      this.boardCache = mergeBoardRefresh(this.boardCache, { wip, swat }, Date.now());
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

  // #4028 — state is derived by chorus-api from the streams and read here;
  // the <role>-declared.json this used to parse no longer exists. blocked and
  // observing (the only states a projection cannot infer) arrive on the same
  // row, so the spine projection below still honours them.
  private applyAndonState(tile: RoleTile, role: string): void {
    if (this.readRolesOverride) {
      const rows = this.readRolesOverride();
      if (rows) this.rolesFromApi = new Map(rows.map((r) => [r.role, r]));
    }
    const row = this.rolesFromApi.get(role);
    if (!row) return; // API not answered yet — tile keeps defaults, the spine projection fills it
    tile.state = row.state || 'idle';
    tile.sessionAlive = row.stale === false;
    if (row.lastActivity) {
      const ageSecs = Math.floor((Date.now() - new Date(row.lastActivity).getTime()) / 1000);
      if (Number.isFinite(ageSecs)) tile.lastActionAge = formatAge(ageSecs); // negative → 'just now', as before
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
      // #4010 — age from the newest observation the PANE would render, not the
      // raw last line. Machinery digests (cards/nudge/role-state/...) are
      // skipped by the streams pane, so aging the tile from one made the two
      // surfaces disagree by construction: tile "3m ago" off a `cards view`
      // call the pane refuses to show, pane newest 11m. Same predicate, both
      // readers — the reconciliation flow (#3976) is the proof it stays that way.
      let last: { ts?: string; digest?: string } | null = null;
      for (const raw of [...lines].reverse()) {
        try {
          const cand = JSON.parse(raw) as { ts?: string; digest?: string };
          if (isRenderableDigest(cand.digest || '')) { last = cand; break; }
        } catch { /* skip unparseable */ }
      }
      if (!last) return;
      tile.lastAction = last.digest || '';
      if (last.ts) {
        const ageSecs = Math.floor((Date.now() - new Date(last.ts).getTime()) / 1000);
        // #4028 — the age label belongs to the same source as the state: the API
        // row's lastActivity. The pane's own clock produced "building · 22m ago".
        if (!this.rolesFromApi.has(role)) tile.lastActionAge = formatAge(ageSecs);
        // #3869 — the observation stream has always known whether a role is
        // working; the tile read it for the age label and then took `state`
        // from a file agents update by hand. That is why every screenshot Jeff
        // sent for two days said "Wren idle" while Wren was mid-build.
        // #4028 — when /api/chorus/context/roles has answered, ITS state stands;
        // the observation age still labels the tile, it no longer re-derives state.
        if (!this.rolesFromApi.has(role)) {
          tile.state = deriveState({ declared: tile.state, lastActivityAgeSecs: ageSecs });
        }
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
    // After the board: the board's card list is authoritative when it answers;
    // the run pin fills card only when the board gave none (board down ≠ idle).
    this.applyWerkRuns(tile, role);
    this.applyLastObservation(tile, role);
    // #3882 — the spine outranks everything above for STATE: Jeff's ruling
    // ("if messages and streams are right then role state largely follows").
    // werk.phase / reply / digest events are the projection input; declared
    // survives only as blocked/observing intent inside the active window.
    this.applySpineProjection(tile, role);
    return tile;
  }

  /** #3882 — read the durable spine's tail once per poll (same file the
   *  streams pane reads, #3884's spinePath). ~400KB covers hours of events. */
  /** #3982 — read through the SINGLE projection, not a second private read.
   *
   *  This method used to open the spine itself and take the last 400 KB, while
   *  the streams pane took 8 MB through spine-tail. Two windows, two accept
   *  rules, one file — which is exactly why a tile could say "building 45s ago"
   *  about a role whose pane showed nothing, 36 minutes stale (2026-08-22 06:19).
   *
   *  Now both come from projectSpine(). There is no second read left to drift. */
  private readSpineActivity(now: number): Record<string, SpineActivity> {
    try {
      return projectSpine(fs, this.spineFile, 80, now, AGENT_ROLES).activity;
    } catch {
      return {};
    }
  }

  private applySpineProjection(tile: RoleTile, role: string): void {
    if (this.rolesFromApi.has(role)) return; // #4028 — one derivation, served by the API
    const act = this.spineActivity[role];
    const projected = projectRoleState({
      declared: tile.state,
      lastEventAgeSecs: act ? act.ageSecs : null,
      lastEventKind: act ? act.kind : null,
      now: Date.now(),
    });
    // 'unknown' from the spine must not erase richer sources (run pins,
    // observations) that already proved presence this poll.
    if (projected.state === 'unknown' && tile.state !== 'idle') return;
    tile.state = projected.state === 'unknown' ? tile.state : projected.state;
    if (projected.ageSecs !== null) tile.lastActionAge = formatAge(projected.ageSecs);
  }

  // #3772 — presence derives from live werk phase, not just last-declared state.
  // A role with a pipeline in flight never shows bare "idle": the run pin
  // (~/.chorus/werk-runs/<card>.json) is ground truth for "is this role working".
  // Declared non-idle states (blocked, observing, …) are NOT overridden — only
  // the idle/unknown lie is corrected.
  //
  // #3780 — a pin's PHASE alone cannot distinguish a live run from an abandoned
  // one: pins reconcile only on poll (#3664), and ~190 pins from long-finished
  // cards sat at "running" the day #3772 shipped — inverting the lie (idle
  // roles showed building off June's cards).
  //
  // #3781 — and pid EXISTENCE is not IDENTITY: pin 3467 (June 17) carried pid
  // 42578, alive on 2026-08-06 as a Chrome renderer started July 23. On a box
  // up 15 days, stale pins collide with reused pids. "Running" now requires
  // the process's ps-reported start time to match the pin's startedAt within
  // 15min — a reused pid always mismatches (pin says June, process says July).
  // One batched `ps` per poll; a pin failing identity is ignored, never
  // trusted-by-default.
  private applyWerkRuns(tile: RoleTile, role: string): void {
    try {
      const files = fs.readdirSync(this.werkRunsDir).filter((f) => /^\d+\.json$/.test(f));
      const pidStarts = this.pidStartTimes();
      for (const f of files) {
        const run = this.readRun(f, role);
        if (!run || !this.runIsCurrent(run, pidStarts)) continue;
        // #4028 — the API row already knows a presented demo (waiting); a run pin
        // only fills state when no row has answered.
        if (!this.rolesFromApi.has(role)
          && (tile.state === 'idle' || tile.state === 'unknown' || tile.state === 'waiting')) {
          tile.state = run.phase === 'presented' ? 'presenting' : 'building';
        }
        tile.sessionAlive = true;
        if (!tile.card && run.card) tile.card = `#${run.card}`;
        return;
      }
    } catch {
      // No werk-runs dir — tiles fall back to declared state alone.
    }
  }

  /** One run pin, if it parses and belongs to this role. Unreadable or foreign → null. */
  private readRun(file: string, role: string): WerkRunPin | null {
    let run: WerkRunPin;
    try {
      run = JSON.parse(fs.readFileSync(path.join(this.werkRunsDir, file), 'utf-8')) as WerkRunPin;
    } catch { return null; }
    return (run.role || '').toLowerCase() === role.toLowerCase() ? run : null;
  }

  /**
   * Does this pin describe something happening NOW?
   *
   * The two phases fail differently, which is why they are asked differently.
   * A `running` pin is only current if a live process matches it by IDENTITY —
   * pid AND start time within 15 minutes — because pids are reused: #3781 caught
   * pin 3467's pid alive as a Chrome renderer, and the tile cheerfully reported a
   * long-dead run as building. A `presented` pin has no process to check, so it
   * ages out instead. Either way a pin that cannot prove itself is ignored, never
   * trusted-by-default.
   */
  private runIsCurrent(run: WerkRunPin, pidStarts: Map<number, number>): boolean {
    if (run.phase === 'running') {
      if (!run.pid || !run.startedAt) return false;
      const procStart = pidStarts.get(run.pid);
      if (procStart === undefined) return false; // pid not alive
      const pinStart = Date.parse(run.startedAt);
      return Number.isFinite(pinStart) && Math.abs(procStart - pinStart) <= 15 * 60 * 1000;
    }
    if (run.phase === 'presented') {
      const t = run.presentedAt ? Date.parse(run.presentedAt) : NaN;
      return Number.isFinite(t) && Date.now() - t <= 24 * 3600 * 1000;
    }
    return false;
  }

  // #3781 — the batched pid→start-time map; cached per poll cycle (5s cadence,
  // one execSync of ps = milliseconds). Test override wins.
  private pidStartTimes(): Map<number, number> {
    if (_pidStartsOverride) return _pidStartsOverride;
    const map = new Map<number, number>();
    try {
      const { execSync } = require('child_process');
      const out: string = execSync('ps -eo pid=,lstart=', { encoding: 'utf-8', timeout: 3000 });
      for (const line of out.split('\n')) {
        const m = line.match(/^\s*(\d+)\s+(.+)$/);
        if (!m) continue;
        const t = Date.parse(m[2].trim());
        if (Number.isFinite(t)) map.set(Number(m[1]), t);
      }
    } catch {
      // ps failed — empty map means every running pin fails identity, which is
      // fail-CLOSED: no pin trusted when the instrument is blind.
    }
    return map;
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

// #3781 — pid → process start time (epoch ms) for every live process, one
// batched `ps` call. Injectable for tests (setPidStartTimesForTest). lstart is
// locale-stable ("Thu Jul 23 11:02:00 2026") and Date.parse handles it.
let _pidStartsOverride: Map<number, number> | null = null;
export function setPidStartTimesForTest(m: Map<number, number> | null): void {
  _pidStartsOverride = m;
}

function formatAge(secs: number): string {
  if (secs < 0) return 'just now';
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}
