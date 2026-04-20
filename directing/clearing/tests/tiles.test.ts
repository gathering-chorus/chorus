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
const OPTS = { scanDir: TMP, pulseFile: PULSE };

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
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch {}
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

describe('TilePoller — role declared state', () => {
  beforeEach(() => { clear(); });

  test('building state surfaces card prefix and session alive', () => {
    writeState('kade', {
      state: 'building',
      card: '2167',
      session_alive: true,
      ts: Math.floor(Date.now() / 1000) - 30,
    });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.state).toBe('building');
    expect(t.card).toBe('#2167');
    expect(t.sessionAlive).toBe(true);
    expect(t.lastActionAge).toMatch(/\d+s ago/);
  });

  test('session_alive:false is respected (default true)', () => {
    writeState('kade', { state: 'idle', session_alive: false });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.sessionAlive).toBe(false);
  });

  test('reconciler divergence surfaces declared vs inferred', () => {
    writeState('silas', { state: 'building', card: '2200', session_alive: true, ts: Math.floor(Date.now() / 1000) });
    writePulse({ roles: { silas: { divergent: true, card_declared: 2100, card_inferred: 2200 } } });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'silas')!;
    expect(t.cardDeclared).toBe('2100');
    expect(t.cardInferred).toBe('2200');
    expect(t.divergent).toBe(true);
  });

  test('reconciler with matching declared=inferred is non-divergent', () => {
    writeState('wren', { state: 'building', card: '50', session_alive: true, ts: Math.floor(Date.now() / 1000) });
    writePulse({ roles: { wren: { divergent: false, card_declared: 50, card_inferred: 50 } } });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'wren')!;
    expect(t.divergent).toBe(false);
  });

  test('malformed state JSON is tolerated (tile stays at defaults)', () => {
    fs.writeFileSync(path.join(TMP, 'kade-declared.json'), 'not json');
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.state).toBe('idle');
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

describe('TilePoller — clearCard', () => {
  beforeEach(() => { clear(); });

  test('flips state to idle and removes card fields', () => {
    writeState('kade', {
      state: 'building', card: '2167', card_type: 'enhance', ts: 1000,
    });
    new TilePoller(OPTS).clearCard('kade');
    const after = JSON.parse(fs.readFileSync(path.join(TMP, 'kade-declared.json'), 'utf-8'));
    expect(after.state).toBe('idle');
    expect(after.card).toBeUndefined();
    expect(after.card_type).toBeUndefined();
    expect(after.ts).toBeGreaterThan(1000);
  });

  test('no-op when state file missing', () => {
    expect(() => new TilePoller(OPTS).clearCard('nobody')).not.toThrow();
  });
});

describe('TilePoller — formatAge via lastActionAge', () => {
  beforeEach(() => { clear(); });

  test.each([
    [5, /\d+s ago/],
    [120, /\d+m ago/],
    [3700, /\d+h ago/],
    [90_000, /\d+d ago/],
  ])('age %d → matches %s', (secs, re) => {
    writeState('kade', { state: 'building', ts: Math.floor(Date.now() / 1000) - secs });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
    expect(t.lastActionAge).toMatch(re);
  });

  test('future ts (negative age) renders as "just now"', () => {
    writeState('kade', { state: 'building', ts: Math.floor(Date.now() / 1000) + 60 });
    const t = new TilePoller(OPTS).getTiles().find((x) => x.role === 'kade')!;
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

  test('second poll picks up new state', () => {
    const p = new TilePoller(OPTS);
    expect(p.getTiles().find((t) => t.role === 'kade')!.state).toBe('idle');
    writeState('kade', { state: 'building', card: '99' });
    p.poll();
    expect(p.getTiles().find((t) => t.role === 'kade')!.state).toBe('building');
  });
});
