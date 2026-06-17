// @test-type: integration — #3442 backfill: touches a real tmpdir (mkdtemp), not unit.
/**
 * hooks-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/hooks-summary.ts. Uses tempdir + env seams for the
 * three log paths (CHORUS_LOG_PATH, PERMISSIONS_LOG_PATH, COMMAND_ERRORS_LOG_PATH).
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { readFileTail } from '../src/lib/log-reader';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'hooks-summary-test-'));
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.PERMISSIONS_LOG_PATH = path.join(TMP, 'permissions.log');
process.env.COMMAND_ERRORS_LOG_PATH = path.join(TMP, 'errors.log');
process.env.HANDOFFS_LOG_PATH = path.join(TMP, 'handoffs.log');

function load() {
  return require('../src/hooks-summary');
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

function writeChorusLog(events: any[]) {
  fs.writeFileSync(
    process.env.CHORUS_LOG_PATH!,
    events.map((e) => JSON.stringify(e)).join('\n') + '\n',
  );
}
function writePermsLog(events: any[]) {
  fs.writeFileSync(
    process.env.PERMISSIONS_LOG_PATH!,
    events.map((e) => JSON.stringify(e)).join('\n') + '\n',
  );
}
function writeErrorsLog(events: any[]) {
  fs.writeFileSync(
    process.env.COMMAND_ERRORS_LOG_PATH!,
    events.map((e) => JSON.stringify(e)).join('\n') + '\n',
  );
}
function clear() {
  for (const f of fs.readdirSync(TMP)) fs.unlinkSync(path.join(TMP, f));
}

const today = new Date().toISOString().slice(0, 10);
const todayTs = `${today}T10:00:00Z`;

describe('getHooksSummary — categories and totals shape', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('empty logs produce all-zero summary with 13 category entries', () => {
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    expect(r.summaries).toHaveLength(13);
    expect(r.totals).toEqual({ today: 0, last7d: 0, blocks: 0, flags: 0, nudges: 0 });
  });

  test('totals aggregate across all summaries', () => {
    writeChorusLog([
      { event: 'decision.gate.matched', timestamp: todayTs, role: 'kade', pref: '001', question: 'should I?' },
      { event: 'decision.gate.matched', timestamp: todayTs, role: 'kade', pref: '002', question: 'continue?' },
    ]);
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    expect(r.totals.today).toBe(2);
    expect(r.totals.last7d).toBe(2);
    expect(r.totals.flags).toBe(2);
  });
});

describe('getHooksSummary — classifier dispatch per event type', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test.each([
    ['search.hierarchy.filesystem_used', 'search-hierarchy', 'flag'],
    ['decision.gate.matched', 'decision-gate', 'flag'],
    ['decision.gate.pass', 'decision-gate', 'allow'],
    ['decision.gate.text_leak', 'jdi-gate', 'block'],
    ['decision.gate.jdi_override', 'jdi-gate', 'allow'],
    ['guard.sparql.warned', 'sparql-guard', 'flag'],
    ['role.nudge.delivered', 'nudge', 'nudge'],
    ['role.nudge.sent', 'nudge', 'nudge'],
    ['build.precommit.completed', 'build-gate', 'log'],
    ['build.commit.created', 'build-gate', 'log'],
    ['card.quality.blocked', 'card-quality', 'block'],
    ['card.quality.warned', 'card-quality', 'flag'],
    ['card.blast_radius.failed', 'card-quality', 'block'],
    ['build.queue.blocked', 'build-gate', 'block'],
    ['build.prepush.started', 'build-gate', 'log'],
    ['deploy.pipeline.skipped', 'deploy-gate', 'flag'],
    ['ops.alert.fired', 'ops-health', 'flag'],
    ['ops.alert.resolved', 'ops-health', 'allow'],
  ])('event %s → category %s, action %s', (event, expectedCategory, expectedAction) => {
    writeChorusLog([{ event, timestamp: todayTs, role: 'kade', pattern: 'x', query: 'q' }]);
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    const cat = r.summaries.find((s: any) => s.category === expectedCategory);
    expect(cat).toBeDefined();
    expect(cat.today).toBe(1);
    expect(cat.recent).toHaveLength(1);
    expect(cat.recent[0].action).toBe(expectedAction);
  });

  test('guard.rule.decided with deny → app-state-guard block', () => {
    writeChorusLog([{
      event: 'guard.rule.decided', timestamp: todayTs, role: 'silas',
      pattern: 'kill -9', decision: 'deny', command: 'kill -9 12345',
    }]);
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    const cat = r.summaries.find((s: any) => s.category === 'app-state-guard');
    expect(cat.today).toBe(1);
    expect(cat.recent[0].action).toBe('block');
    expect(cat.recent[0].detail).toContain('deny');
  });

  test('guard.rule.decided with allow → app-state-guard allow', () => {
    writeChorusLog([{ event: 'guard.rule.decided', timestamp: todayTs, pattern: 'p', decision: 'allow', command: 'ok' }]);
    const { getHooksSummary } = load();
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'app-state-guard');
    expect(cat.recent[0].action).toBe('allow');
  });

  test('guard.classify.decided dispatches deny→block, ask→flag, allow→allow', () => {
    writeChorusLog([
      { event: 'guard.classify.decided', timestamp: todayTs, decision: 'deny', path: '/secret' },
      { event: 'guard.classify.decided', timestamp: todayTs, decision: 'ask', path: '/suspect' },
      { event: 'guard.classify.decided', timestamp: todayTs, decision: 'allow', path: '/ok' },
    ]);
    const { getHooksSummary } = load();
    const cat = getHooksSummary().summaries.find((s: any) => s.category === 'sensitive-paths');
    expect(cat.today).toBe(3);
    const actions = cat.recent.map((e: any) => e.action).sort();
    expect(actions).toEqual(['allow', 'block', 'flag']);
  });

  test('guard.scrub.blocked action inherits from decision field', () => {
    writeChorusLog([
      { event: 'guard.scrub.blocked', timestamp: todayTs, decision: 'warn', path: '/env' },
      { event: 'guard.scrub.blocked', timestamp: todayTs, decision: 'block', path: '/secret' },
    ]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'credential-guard');
    expect(cat.today).toBe(2);
    expect(cat.recent.some((e: any) => e.action === 'flag')).toBe(true);
    expect(cat.recent.some((e: any) => e.action === 'block')).toBe(true);
  });

  test('unknown events are skipped, not misclassified', () => {
    writeChorusLog([
      { event: 'totally.unknown.event', timestamp: todayTs, role: 'kade' },
      { event: 'card.quality.blocked', timestamp: todayTs, role: 'kade', reason: 'x' },
    ]);
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    expect(r.totals.today).toBe(1);
  });

  test('nudge subtype without delivered/sent becomes log action', () => {
    writeChorusLog([{ event: 'nudge.other', timestamp: todayTs, role: 'kade' }]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'nudge');
    expect(cat.recent[0].action).toBe('log');
  });
});

describe('getHooksSummary — time-window filters', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('today counts only events whose timestamp starts with today', () => {
    const yesterday = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
    writeChorusLog([
      { event: 'card.quality.blocked', timestamp: `${yesterday}T10:00:00Z`, reason: 'old' },
      { event: 'card.quality.blocked', timestamp: todayTs, reason: 'fresh' },
    ]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'card-quality');
    expect(cat.today).toBe(1);
    expect(cat.last7d).toBe(2);
  });

  test('events older than 7 days do not count toward last7d', () => {
    const old = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000).toISOString();
    writeChorusLog([
      { event: 'card.quality.blocked', timestamp: old, reason: 'ancient' },
    ]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'card-quality');
    expect(cat.today).toBe(0);
    expect(cat.last7d).toBe(0);
  });

  test('recent field keeps the last 5 events reversed (newest first)', () => {
    const events: any[] = [];
    for (let i = 1; i <= 7; i++) {
      events.push({
        event: 'role.nudge.sent',
        timestamp: `${today}T10:0${i}:00Z`,
        role: 'kade',
      });
    }
    writeChorusLog(events);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'nudge');
    expect(cat.recent).toHaveLength(5);
    // Newest first per .slice(-5).reverse()
    expect(cat.recent[0].timestamp).toBe(`${today}T10:07:00Z`);
    expect(cat.recent[4].timestamp).toBe(`${today}T10:03:00Z`);
  });
});

describe('getHooksSummary — permissions log', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('permissions.log entries show up under permission-logger', () => {
    writePermsLog([
      { timestamp: todayTs, role: 'kade', tool: 'Bash', detail: 'ls /tmp' },
      { timestamp: todayTs, role: 'silas', tool: 'Read', detail: 'long-content'.repeat(20) },
    ]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'permission-logger');
    expect(cat.today).toBe(2);
    expect(cat.recent[0].action).toBe('log');
    // Detail truncation
    expect(cat.recent.every((e: any) => e.detail.length <= 128)).toBe(true);
  });

  test('malformed permissions lines are skipped', () => {
    fs.writeFileSync(process.env.PERMISSIONS_LOG_PATH!, 'garbage\n');
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'permission-logger');
    expect(cat.today).toBe(0);
  });
});

describe('getHooksSummary — errors log DOCKER/KILL fingerprints', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('DOCKER_BLOCKED and KILL_BLOCKED entries surface as app-state-guard blocks', () => {
    writeErrorsLog([
      { fingerprint: 'DOCKER_BLOCKED', ts: todayTs, role: 'kade', cmd: 'docker rm -f x' },
      { fingerprint: 'KILL_BLOCKED', ts: todayTs, role: 'silas', cmd: 'kill -9 99' },
      { fingerprint: 'OTHER', ts: todayTs, role: 'x', cmd: 'ignored' },  // different FP, skipped
    ]);
    const cat = load().getHooksSummary().summaries.find((s: any) => s.category === 'app-state-guard');
    expect(cat.today).toBe(2);
    expect(cat.recent.every((e: any) => e.action === 'block')).toBe(true);
  });

  test('malformed errors lines are skipped', () => {
    fs.writeFileSync(process.env.COMMAND_ERRORS_LOG_PATH!, 'not-json\n');
    const { getHooksSummary } = load();
    expect(() => getHooksSummary()).not.toThrow();
  });
});

describe('getHooksSummary — robustness', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('missing log files are tolerated (all counters stay 0)', () => {
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    expect(r.totals.today).toBe(0);
  });

  test('malformed chorus log line is skipped', () => {
    fs.writeFileSync(process.env.CHORUS_LOG_PATH!, 'not json\n' + JSON.stringify({
      event: 'card.quality.blocked', timestamp: todayTs, reason: 'good',
    }) + '\n');
    const { getHooksSummary } = load();
    const r = getHooksSummary();
    expect(r.totals.today).toBe(1);
  });

  test('classifier returning null is honored (no event added)', () => {
    // decision.gate.matched always returns an object — no null-returning classifier
    // in current code. Use an event whose match fn never fires to exercise the
    // "no classifier matches" fall-through.
    writeChorusLog([{ event: 'not.in.any.classifier', timestamp: todayTs }]);
    const r = load().getHooksSummary();
    expect(r.totals.today).toBe(0);
  });
});

// #3406 sibling-audit — hooks-summary read the whole 535MB chorus.log synchronously
// (freeze/OOM-crash class). It now reads a bounded tail via readFileTail. With a tiny
// tail, today-events before the tail are NOT counted — proving the read is bounded.
describe('#3406 — bounded chorus.log read', () => {
  beforeEach(() => { clear(); jest.resetModules(); });
  afterEach(() => { delete process.env.CHORUS_LOG_TAIL_BYTES; jest.resetModules(); });

  test('readFileTail bounds the read, and the summary counts only the tail', () => {
    process.env.CHORUS_LOG_TAIL_BYTES = '500';
    const now = new Date().toISOString();
    // 20 today decision.gate.matched at the START (beyond a 500B tail) + 3 at the END.
    // Unbounded, all 23 count toward totals.today.
    const start = Array.from({ length: 20 }, (_, i) => ({ event: 'decision.gate.matched', timestamp: now, role: 'kade', pref: `s${i}`, question: 'q' }));
    const end = Array.from({ length: 3 }, (_, i) => ({ event: 'decision.gate.matched', timestamp: now, role: 'wren', pref: `e${i}`, question: 'q' }));
    writeChorusLog([...start, ...end]);
    // the bounding mechanism itself returns at most maxBytes, not the whole file
    const tail = readFileTail(process.env.CHORUS_LOG_PATH!, 500);
    expect(tail).not.toBeNull();
    expect(tail!.length).toBeLessThanOrEqual(500);
    // end-to-end: only the tail's events are counted, not all 23
    jest.resetModules();
    const total = load().getHooksSummary().totals.today;
    expect(total).toBeGreaterThan(0);
    expect(total).toBeLessThan(23);
  });
});
