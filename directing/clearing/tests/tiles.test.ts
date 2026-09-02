// @test-type: unit — tmp fixture dirs (scan/pulse/werk-runs), no live services; brings its own world.
/**
 * TilePoller — unit tests (#2167 phase 2).
 *
 * Target: 80%+ on src/tiles.ts. fs reads go against a tempdir fixture
 * passed via TilePollerOptions (scanDir, pulseFile) — no env var hacks,
 * no dynamic require, no jest.resetModules (#2273).
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { TilePoller } from '../src/tiles';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles-test-'));
const PULSE = path.join(TMP, 'pulse-latest.json');
// #3772: werkRunsDir pinned to an empty fixture dir — without it the poller
// reads the LIVE ~/.chorus/werk-runs and a real in-flight pipeline leaks into
// these hermetic tests (a test brings its own world, #3528).
const WERK_TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles-werk-'));
const OPTS = { scanDir: TMP, pulseFile: PULSE, werkRunsDir: WERK_TMP };

function writeState(role: string, data: any) {
  fs.writeFileSync(path.join(TMP, `${role}-declared.json`), JSON.stringify(data));
}
function writeObs(role: string, lines: any[]) {
  const body = lines.map((l) => JSON.stringify(l)).join('\n') + '\n';
  fs.writeFileSync(path.join(TMP, `${role}-observations.jsonl`), body);
}
function writeJeff(data: any) {
  fs.writeFileSync(path.join(TMP, 'jeff-input.json'), JSON.stringify(data));
}
function writePulse(data: any) {
  fs.writeFileSync(PULSE, JSON.stringify(data));
}
function clear() {
  for (const f of fs.readdirSync(TMP)) fs.unlinkSync(path.join(TMP, f));
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

describe('TilePoller — constructor initializes roles', () => {
  beforeEach(() => { clear(); });

  test('getTiles returns four roles in order', () => {
    const p = new TilePoller(OPTS);
    const tiles = p.getTiles();
    expect(tiles.map((t: any) => t.role)).toEqual(['jeff', 'wren', 'silas', 'kade']);
  });

  test('fresh tiles with no state files have idle/offline defaults', () => {
    const p = new TilePoller(OPTS);
    const tiles = p.getTiles();
    const wren = tiles.find((t) => t.role === 'wren')!;
    const jeff = tiles.find((t) => t.role === 'jeff')!;
    expect(wren.state).toBe('idle');
    expect(wren.card).toBe('');
    expect(wren.sessionAlive).toBe(false);
    expect(jeff.state).toBe('offline');
  });
});

describe('TilePoller — role state comes from the derived endpoint (#4028)', () => {
  beforeEach(() => { clear(); });

  const rows = (over: Array<Partial<import('../src/tiles').DerivedRoleRow> & { role: string }>) => () =>
    over.map((r) => ({ state: 'idle', detail: null, lastActivity: null, stale: true, ...r }));

  test('building on the derived row surfaces building + session alive; card still comes from the board', () => {
    const p = new TilePoller({ ...OPTS, readRoles: rows([
      { role: 'kade', state: 'building', stale: false, lastActivity: new Date(Date.now() - 30_000).toISOString() },
    ]) });
    const t = p.getTiles().find((x) => x.role === 'kade')!;
    expect(t.state).toBe('building');
    expect(t.card).toBe('');
    expect(t.sessionAlive).toBe(true);
    expect(t.lastActionAge).toMatch(/\d+s ago/);
  });

  test('a stale derived row reads as session not alive', () => {
    const p = new TilePoller({ ...OPTS, readRoles: rows([{ role: 'kade', state: 'idle', stale: true }]) });
    expect(p.getTiles().find((x) => x.role === 'kade')!.sessionAlive).toBe(false);
  });

  test('blocked arrives on the row and the tile shows it', () => {
    const p = new TilePoller({ ...OPTS, readRoles: rows([
      { role: 'wren', state: 'blocked', detail: 'waiting on Jeff', stale: false, lastActivity: new Date().toISOString() },
    ]) });
    expect(p.getTiles().find((x) => x.role === 'wren')!.state).toBe('blocked');
  });

  test('negative proof (#3734): a <role>-declared.json on disk is NOT read — the tile ignores it', () => {
    writeState('kade', { state: 'building', session_alive: true, ts: Math.floor(Date.now() / 1000) - 30 });
    const p = new TilePoller({ ...OPTS, readRoles: rows([{ role: 'kade', state: 'idle', stale: true }]) });
    const t = p.getTiles().find((x) => x.role === 'kade')!;
    expect(t.state).toBe('idle');
    expect(t.sessionAlive).toBe(false);
  });

  test('reconciler divergence is non-applicable (no declared card to diverge from)', () => {
    writePulse({ roles: { silas: { divergent: true, card_declared: 2100, card_inferred: 2200 } } });
    const p = new TilePoller({ ...OPTS, readRoles: rows([{ role: 'silas', state: 'building', stale: false }]) });
    const t = p.getTiles().find((x) => x.role === 'silas')!;
    expect(t.divergent).toBe(false);
    expect(t.cardDeclared).toBeUndefined();
    expect(t.cardInferred).toBeUndefined();
  });

  test('no derived rows yet (API not answered) — tile keeps defaults', () => {
    const p = new TilePoller({ ...OPTS, readRoles: () => null });
    expect(p.getTiles().find((x) => x.role === 'kade')!.state).toBe('idle');
  });
});

describe('TilePoller — observations and lastAction', () => {
  beforeEach(() => { clear(); });

  test('last observation digest becomes lastAction, ts becomes age', () => {
    const past = new Date(Date.now() - 120_000).toISOString();
    writeObs('kade', [
      { digest: 'old', ts: new Date(Date.now() - 600_000).toISOString() },
      { digest: 'fresh digest', ts: past },
    ]);
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.lastAction).toBe('fresh digest');
    expect(t.lastActionAge).toMatch(/\dm ago|\d+s ago/);
  });

  test('empty observations file is tolerated', () => {
    fs.writeFileSync(path.join(TMP, 'kade-observations.jsonl'), '');
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.lastAction).toBe('');
  });
});

describe('TilePoller — jeff tile state machine', () => {
  beforeEach(() => { clear(); });

  test('away when no recent update (>5min)', () => {
    writeJeff({ updated: Math.floor(Date.now() / 1000) - 1000 });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('away');
    expect(t.sessionAlive).toBe(false);
  });

  test('directing when typing (keys_per_min > 0)', () => {
    writeJeff({
      updated: Math.floor(Date.now() / 1000) - 10,
      keys_per_min: 80,
    });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('directing');
    expect(t.sessionAlive).toBe(true);
    expect(t.lastAction).toContain('80 keys/min');
  });

  test('watching when clicking but not typing', () => {
    writeJeff({
      updated: Math.floor(Date.now() / 1000) - 20,
      clicks_per_min: 12,
    });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('watching');
  });

  test('watching when mouse_active but not typing or clicking', () => {
    writeJeff({
      updated: Math.floor(Date.now() / 1000) - 20,
      mouse_active: true,
    });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('watching');
  });

  test('present when recent update with no activity metrics', () => {
    writeJeff({
      updated: Math.floor(Date.now() / 1000) - 30,
    });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('present');
  });

  test('missing jeff-input.json leaves jeff offline', () => {
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'jeff')!;
    expect(t.state).toBe('offline');
  });
});

describe('TilePoller — pulse', () => {
  beforeEach(() => { clear(); });

  test('getPulse returns null when no pulse file', () => {
    expect(new TilePoller(OPTS).getPulse()).toBeNull();
  });

  test('getPulse maps fields from pulse-latest.json', () => {
    writePulse({
      alerts: { count: 3 },
      index_freshness: { fresh: 10, warn: 2, critical: 1, dead: 0 },
      nudges: { kade: { pending: 1, stale: false } },
      events: { last_60s_count: 42 },
      elapsed_ms: 681,
    });
    const p = new TilePoller(OPTS).getPulse()!;
    expect(p.alertsToday).toBe(3);
    expect(p.indexFreshness.critical).toBe(1);
    expect(p.nudges.kade.pending).toBe(1);
    expect(p.eventsLast60s).toBe(42);
    expect(p.elapsed_ms).toBe(681);
  });

  test('getPulse applies defaults for missing fields', () => {
    writePulse({});
    const p = new TilePoller(OPTS).getPulse()!;
    expect(p.alertsToday).toBe(0);
    expect(p.indexFreshness).toEqual({ fresh: 0, warn: 0, critical: 0, dead: 0 });
    expect(p.eventsLast60s).toBe(0);
  });
});

// #2467: clearCard test suite retired. Card is no longer in role-state;
// tile renderer reads cards directly from the board (boardCache), which
// reflects card.accepted automatically. No state mutation needed.

describe('TilePoller — formatAge via lastActionAge', () => {
  beforeEach(() => { clear(); });

  test.each([
    [5, /\d+s ago/],
    [120, /\d+m ago/],
    [3700, /\d+h ago/],
    [90_000, /\d+d ago/],
  ])('age %d → matches %s', (secs, re) => {
    const readRoles = () => [{ role: 'kade', state: 'building', stale: false, lastActivity: new Date(Date.now() - secs * 1000).toISOString() }];
    const t = new TilePoller({ ...OPTS, readRoles }).getTiles().find((x) => x.role === 'kade')!;
    expect(t.lastActionAge).toMatch(re);
  });

  test('future lastActivity (negative age) renders as "just now"', () => {
    const readRoles = () => [{ role: 'kade', state: 'building', stale: false, lastActivity: new Date(Date.now() + 60_000).toISOString() }];
    const t = new TilePoller({ ...OPTS, readRoles }).getTiles().find((x) => x.role === 'kade')!;
    expect(t.lastActionAge).toBe('just now');
  });
});

describe('TilePoller — board refresh resilience', () => {
  let originalFetch: any;
  beforeEach(() => { clear(); originalFetch = (global as any).fetch; });
  afterEach(() => { (global as any).fetch = originalFetch; });

  test('WIP cards render when swat endpoint rejects', async () => {
    const wipData = { data: { cards: [{ id: 123, owner: 'wren', title: 'test card' }] } };
    (global as any).fetch = jest.fn().mockImplementation((url: string) => {
      if ((url as string).includes('/swat')) return Promise.reject(new Error('swat down'));
      return Promise.resolve({ ok: true, json: () => Promise.resolve(wipData) });
    });
    writeState('wren', { state: 'building', card: '123', session_alive: true, ts: Math.floor(Date.now() / 1000) });
    writePulse({});
    const p = new TilePoller(OPTS);
    await p.boardRefresh; // #2273: await explicit promise instead of setTimeout flush
    p.poll();
    const wren = p.getTiles().find((t) => t.role === 'wren')!;
    expect(wren.cards).toContain('#123');
  });
});

describe('TilePoller — poll re-reads state', () => {
  beforeEach(() => { clear(); });

  test('second poll picks up the new derived state', () => {
    let rows: any[] = [];
    const p = new TilePoller({ ...OPTS, readRoles: () => rows });
    expect(p.getTiles().find((t) => t.role === 'kade')!.state).toBe('idle');
    rows = [{ role: 'kade', state: 'building', stale: false, lastActivity: new Date().toISOString() }];
    p.poll();
    expect(p.getTiles().find((t) => t.role === 'kade')!.state).toBe('building');
  });
});
