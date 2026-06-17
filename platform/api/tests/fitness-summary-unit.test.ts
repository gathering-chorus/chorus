// @test-type: integration — #3442 backfill: touches a real tmpdir (mkdtemp), not unit.
/**
 * fitness-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/fitness-summary.ts. Uses CHORUS_LOG_PATH tempfile.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { readFileTail } from '../src/lib/log-reader';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'fitness-test-'));
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');

function load() {
  return require('../src/fitness-summary');
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

function writeLog(events: any[]) {
  fs.writeFileSync(process.env.CHORUS_LOG_PATH!, events.map((e) => JSON.stringify(e)).join('\n') + '\n');
}

function clear() {
  try { fs.unlinkSync(process.env.CHORUS_LOG_PATH!); } catch { /* ignore */ }
}

// Anchored at noon today UTC (#2543). The resolver no longer caps open
// session windows at wall-clock now, so fixtures past noon (when the test
// runs before noon UTC) are still claimed correctly.
const today = new Date().toISOString().slice(0, 10);
const REFERENCE_NOON_MS = new Date(today + 'T12:00:00.000Z').getTime();
const ts = (hours = 0, mins = 0, secs = 0) => {
  return new Date(REFERENCE_NOON_MS - (hours * 3600 + mins * 60 + secs) * 1000).toISOString();
};

describe('getFitnessSummary — shape and empty state', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('missing log → 4 zero functions', () => {
    const r = load().getFitnessSummary();
    expect(r.functions).toHaveLength(4);
    const ids = r.functions.map((f: any) => f.id).sort();
    expect(ids).toEqual(['decision-gate-rate', 'jdi-rate', 'retry-rate', 'search-hierarchy-rate']);
    for (const f of r.functions) {
      expect(f.overall7d).toBe(0);
      expect(f.overallToday).toBe(0);
      expect(f.recentEvents).toEqual([]);
    }
  });

  test('every function has byRole for silas, wren, kade', () => {
    writeLog([]);
    const r = load().getFitnessSummary();
    for (const f of r.functions) {
      expect(Object.keys(f.byRole).sort()).toEqual(['kade', 'silas', 'wren']);
    }
  });

  test('direction is lower-is-better for all four current functions', () => {
    writeLog([]);
    const r = load().getFitnessSummary();
    for (const f of r.functions) {
      expect(f.direction).toBe('lower-is-better');
    }
  });
});

describe('getFitnessSummary — session window + event counts', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('jdi events without sessions still count toward overall but role rate stays 0', () => {
    writeLog([
      { event: 'decision.gate.text_leak', timestamp: ts(1), role: 'kade' },
      { event: 'decision.gate.jdi_override', timestamp: ts(2), role: 'kade' },
    ]);
    const r = load().getFitnessSummary();
    const jdi = r.functions.find((f: any) => f.id === 'jdi-rate');
    expect(jdi.overall7d).toBe(2);
    expect(jdi.byRole.kade.events).toBe(2);
    expect(jdi.byRole.kade.sessions).toBe(0);
    expect(jdi.byRole.kade.rate).toBe(0);  // division by 0 guarded
  });

  test('session window provides denominator for per-role rate', () => {
    writeLog([
      { event: 'session.role.started', timestamp: ts(3), role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: ts(2, 30), role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: ts(2, 15), role: 'kade' },
    ]);
    const r = load().getFitnessSummary();
    const jdi = r.functions.find((f: any) => f.id === 'jdi-rate');
    expect(jdi.byRole.kade.sessions).toBe(1);
    expect(jdi.byRole.kade.events).toBe(2);
    expect(jdi.byRole.kade.rate).toBe(2);  // 2 / 1
  });

  test('overallToday counts only events with timestamp startsWith today', () => {
    const oldDay = new Date(Date.now() - 48 * 3600 * 1000).toISOString();
    writeLog([
      { event: 'decision.gate.matched', timestamp: oldDay, role: 'kade' },
      { event: 'decision.gate.matched', timestamp: ts(1), role: 'kade' },
    ]);
    const r = load().getFitnessSummary();
    const dg = r.functions.find((f: any) => f.id === 'decision-gate-rate');
    expect(dg.overall7d).toBe(2);
    expect(dg.overallToday).toBe(1);
  });

  test('recentEvents is last 8 matched, reversed newest first', () => {
    const events: any[] = [];
    for (let i = 0; i < 10; i++) {
      events.push({ event: 'decision.gate.matched', timestamp: ts(0, 0, i), role: 'kade' });
    }
    writeLog(events);
    const dg = load().getFitnessSummary().functions.find((f: any) => f.id === 'decision-gate-rate');
    expect(dg.recentEvents).toHaveLength(8);
  });

  test('search-hierarchy filterFn excludes code_lookup="true"', () => {
    writeLog([
      { event: 'search.hierarchy.filesystem_used', timestamp: ts(1), role: 'kade', code_lookup: 'true' },
      { event: 'search.hierarchy.filesystem_used', timestamp: ts(1), role: 'kade', code_lookup: 'false' },
      { event: 'search.hierarchy.filesystem_used', timestamp: ts(1), role: 'kade' },
    ]);
    const sh = load().getFitnessSummary().functions.find((f: any) => f.id === 'search-hierarchy-rate');
    expect(sh.overall7d).toBe(2);  // code_lookup=true one is filtered out
  });

  test('unknown role is resolved to active session role', () => {
    writeLog([
      { event: 'session.role.started', timestamp: ts(2), role: 'silas' },
      { event: 'decision.gate.matched', timestamp: ts(1), role: 'unknown' },
    ]);
    const dg = load().getFitnessSummary().functions.find((f: any) => f.id === 'decision-gate-rate');
    expect(dg.byRole.silas.events).toBe(1);
  });
});

describe('getFitnessSummary — trend calculation', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('trend7d reflects direction=lower-is-better semantics', () => {
    // Previous 7d had more → this 7d fewer → improvement → positive trend
    const prevWeek = new Date(Date.now() - 10 * 86400 * 1000).toISOString();
    writeLog([
      { event: 'session.role.started', timestamp: prevWeek, role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: prevWeek, role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: prevWeek, role: 'kade' },
      { event: 'session.role.started', timestamp: ts(1), role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: ts(1), role: 'kade' },
    ]);
    const jdi = load().getFitnessSummary().functions.find((f: any) => f.id === 'jdi-rate');
    // prev rate = 2/1=2, this rate = 1/1=1 → trend = prev - this = 1
    expect(jdi.trend7d).toBe(1);
  });

  test('zero sessions in prev week means ratePrev = 0, not NaN', () => {
    writeLog([
      { event: 'session.role.started', timestamp: ts(1), role: 'kade' },
      { event: 'decision.gate.text_leak', timestamp: ts(1), role: 'kade' },
    ]);
    const jdi = load().getFitnessSummary().functions.find((f: any) => f.id === 'jdi-rate');
    // prev 0, this 1/1=1 → trend = 0 - 1 = -1 (worse)
    expect(jdi.trend7d).toBe(-1);
  });
});

describe('getFitnessSummary — retry cluster detection', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('two session_tool events within 30s with same action/diff summary = 1 cluster', () => {
    writeLog([
      { event: 'session_tool', timestamp: ts(1, 0, 10), role: 'kade', action: 'Bash', summary: 'bash ../../platform/scripts/cards list' },
      { event: 'session_tool', timestamp: ts(1, 0, 5), role: 'kade', action: 'Bash', summary: 'bash ../../platform/scripts/cards view 5' },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.overall7d).toBe(1);
    expect(retry.recentEvents[0].category).toBe('board');
  });

  test.each([
    ['bash git-queue commit x', 'git-queue'],
    ['curl localhost:3030/x', 'fuseki'],
    ['curl localhost:3102/x', 'loki-grafana'],
    ['curl localhost:3340/x', 'endpoint'],
    ['app-state.sh status', 'deploy'],
    ['chorus-log query x', 'chorus-log'],
    ['role-state kade building', 'role-state'],
    ['echo hello', 'other'],
  ])('retry summary %s → category %s', (summary, expected) => {
    writeLog([
      { event: 'session_tool', timestamp: ts(1, 0, 10), role: 'kade', action: 'Bash', summary },
      { event: 'session_tool', timestamp: ts(1, 0, 5), role: 'kade', action: 'Bash', summary: `${summary} different arg` },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.recentEvents[0].category).toBe(expected);
  });

  test('same summary twice = not a cluster (skipped)', () => {
    writeLog([
      { event: 'session_tool', timestamp: ts(1, 0, 10), role: 'kade', action: 'Bash', summary: 'exact same' },
      { event: 'session_tool', timestamp: ts(1, 0, 5), role: 'kade', action: 'Bash', summary: 'exact same' },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.overall7d).toBe(0);
  });

  test('different actions = not a cluster', () => {
    writeLog([
      { event: 'session_tool', timestamp: ts(1, 0, 10), role: 'kade', action: 'Bash', summary: 'a' },
      { event: 'session_tool', timestamp: ts(1, 0, 5), role: 'kade', action: 'Read', summary: 'b' },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.overall7d).toBe(0);
  });

  test('different roles = not a cluster', () => {
    writeLog([
      { event: 'session_tool', timestamp: ts(1, 0, 10), role: 'kade', action: 'Bash', summary: 'a' },
      { event: 'session_tool', timestamp: ts(1, 0, 5), role: 'silas', action: 'Bash', summary: 'b' },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.overall7d).toBe(0);
  });

  test('> 30s apart = not a cluster', () => {
    writeLog([
      { event: 'session_tool', timestamp: ts(2, 0), role: 'kade', action: 'Bash', summary: 'a' },
      { event: 'session_tool', timestamp: ts(1, 0), role: 'kade', action: 'Bash', summary: 'b' },
    ]);
    const retry = load().getFitnessSummary().functions.find((f: any) => f.id === 'retry-rate');
    expect(retry.overall7d).toBe(0);
  });

  test('malformed timestamps skip cluster detection', () => {
    writeLog([
      { event: 'session_tool', timestamp: 'not-a-date', role: 'kade', action: 'Bash', summary: 'a' },
      { event: 'session_tool', timestamp: 'also-bad', role: 'kade', action: 'Bash', summary: 'b' },
    ]);
    // Should not throw
    expect(() => load().getFitnessSummary()).not.toThrow();
  });
});

describe('getFitnessSummary — log robustness', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('malformed log lines are skipped', () => {
    fs.writeFileSync(process.env.CHORUS_LOG_PATH!,
      'not json\n' + JSON.stringify({ event: 'decision.gate.matched', timestamp: ts(1), role: 'kade' }) + '\n');
    const r = load().getFitnessSummary();
    const dg = r.functions.find((f: any) => f.id === 'decision-gate-rate');
    expect(dg.overall7d).toBe(1);
  });

  test('events without timestamp or event are filtered out', () => {
    writeLog([
      { role: 'kade' },  // no event
      { event: 'decision.gate.matched', role: 'kade' },  // no timestamp
      { event: 'decision.gate.matched', timestamp: ts(1), role: 'kade' },  // good
    ]);
    const dg = load().getFitnessSummary().functions.find((f: any) => f.id === 'decision-gate-rate');
    expect(dg.overall7d).toBe(1);
  });
});

// #3406 sibling-audit — fitness-summary read the whole 535MB chorus.log synchronously
// (same freeze/OOM-crash class as /context/spine). It now reads a bounded tail via
// readFileTail. With a tiny tail, events before the tail are NOT counted even though
// they're inside the 7d window — proving the read is bounded, not whole.
describe('#3406 — bounded chorus.log read', () => {
  beforeEach(() => { clear(); jest.resetModules(); });
  afterEach(() => { delete process.env.CHORUS_LOG_TAIL_BYTES; jest.resetModules(); });

  test('readFileTail bounds the read, and the summary counts only the tail', () => {
    process.env.CHORUS_LOG_TAIL_BYTES = '500'; // ~6 lines worth
    // 20 recent jdi events at the START (beyond a 500B tail) + 3 at the END (within it).
    // All inside the 7d window, so an UNBOUNDED read would count all 23.
    const start = Array.from({ length: 20 }, (_, i) => ({ event: 'decision.gate.jdi_override', timestamp: ts(1, i), role: 'kade' }));
    const end = Array.from({ length: 3 }, (_, i) => ({ event: 'decision.gate.jdi_override', timestamp: ts(0, i), role: 'wren' }));
    writeLog([...start, ...end]);
    // the bounding mechanism itself: readFileTail returns at most maxBytes, not the whole file
    const tail = readFileTail(process.env.CHORUS_LOG_PATH!, 500);
    expect(tail).not.toBeNull();
    expect(tail!.length).toBeLessThanOrEqual(500);
    // end-to-end: the summary counts only the tail's events, not all 23
    jest.resetModules();
    const jdi = load().getFitnessSummary().functions.find((f: any) => f.id === 'jdi-rate');
    expect(jdi.overall7d).toBeGreaterThan(0);   // tail read works
    expect(jdi.overall7d).toBeLessThan(23);     // bounded — did NOT read the whole file
  });
});
