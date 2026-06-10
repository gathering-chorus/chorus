/**
 * hook-friction handler — unit tests (#3280).
 *
 * The ranked friction view over hooks.log (#3252 JSON format): which hook
 * blocks whom, how often, per role — DENY/BLOCK/WARN only, allow is not
 * friction. Reads the existing log (AC2: no new store).
 */
import { fetchHookFriction } from '../../src/handlers/hook-friction';

const FIXED_NOW = new Date('2026-06-10T12:00:00-0400').getTime();
const nowFn = () => FIXED_NOW;
const HOUR_MS = 60 * 60 * 1000;

function jline(hoursAgo: number, module: string, role: string, decision: string, extra: object = {}): string {
  const ts = new Date(FIXED_NOW - hoursAgo * HOUR_MS).toISOString();
  return JSON.stringify({
    appName: 'chorus-hooks', component: 'dispatch', decision, hook: 'pre_tool_use',
    latency_ms: 5, module, role, session_id: 'abc123', timestamp: ts, tool: 'Bash', ...extra,
  });
}

type FrictionBody = {
  windowHours: number;
  totalFriction: number;
  hooks: Array<{ module: string; total: number; deny: number; warn: number; byRole: Record<string, number> }>;
  generatedAt: string;
};

describe('fetchHookFriction (#3280)', () => {
  test('readLog null → 503', () => {
    const r = fetchHookFriction({ readLog: () => null, now: nowFn });
    expect(r.status).toBe(503);
  });

  test('empty log → 200 zero friction', () => {
    const r = fetchHookFriction({ readLog: () => '', now: nowFn });
    expect(r.status).toBe(200);
    const body = r.body as FrictionBody;
    expect(body.totalFriction).toBe(0);
    expect(body.hooks).toEqual([]);
  });

  test('AC1: ranked by total friction desc, with per-role breakdown', () => {
    const log = [
      jline(1, 'search_hierarchy', 'kade', 'deny'),
      jline(1, 'accept_gate', 'kade', 'deny'),
      jline(2, 'accept_gate', 'kade', 'deny'),
      jline(2, 'accept_gate', 'silas', 'deny'),
      jline(3, 'memory_first', 'wren', 'warn'),
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    const body = r.body as FrictionBody;
    expect(body.totalFriction).toBe(5);
    // AC3: the hook hammering a role ranks first.
    expect(body.hooks[0].module).toBe('accept_gate');
    expect(body.hooks[0].total).toBe(3);
    expect(body.hooks[0].byRole).toEqual({ kade: 2, silas: 1 });
    expect(body.hooks.map((h) => h.module)).toEqual(['accept_gate', 'search_hierarchy', 'memory_first']);
  });

  test('allow is not friction; deny+block merge; warn counted separately', () => {
    const log = [
      jline(1, 'ops_awareness', 'silas', 'allow'),
      jline(1, 'ops_awareness', 'silas', 'warn'),
      jline(1, 'canonical_guard', 'silas', 'block'),
      jline(1, 'canonical_guard', 'silas', 'deny'),
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    const body = r.body as FrictionBody;
    expect(body.totalFriction).toBe(3);
    const guard = body.hooks.find((h) => h.module === 'canonical_guard');
    expect(guard?.deny).toBe(2); // block + deny are the same refusal class
    const ops = body.hooks.find((h) => h.module === 'ops_awareness');
    expect(ops?.warn).toBe(1);
    expect(ops?.total).toBe(1);
  });

  test('window cutoff: events outside windowHours are excluded', () => {
    const log = [
      jline(1, 'accept_gate', 'kade', 'deny'),
      jline(13, 'accept_gate', 'kade', 'deny'), // outside default 12h
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    const body = r.body as FrictionBody;
    expect(body.windowHours).toBe(12);
    expect(body.totalFriction).toBe(1);
  });

  test('windowHours param widens the window', () => {
    const log = [
      jline(1, 'accept_gate', 'kade', 'deny'),
      jline(13, 'accept_gate', 'kade', 'deny'),
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn, windowHours: 24 });
    const body = r.body as FrictionBody;
    expect(body.windowHours).toBe(24);
    expect(body.totalFriction).toBe(2);
  });

  test('tolerates legacy pipe-format and garbage lines (skip, never throw)', () => {
    const log = [
      '2026-06-10T11:00:00Z | pre_tool_use | Bash | kade | accept_gate | deny | 5ms | s1 | ctx',
      'not json at all',
      jline(1, 'accept_gate', 'kade', 'deny'),
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    expect(r.status).toBe(200);
    expect((r.body as FrictionBody).totalFriction).toBe(1);
  });

  test('module none/empty rows are skipped (allow-path noise)', () => {
    const log = [
      jline(1, 'none', 'kade', 'deny'),
      jline(1, '', 'kade', 'deny'),
      jline(1, 'accept_gate', 'kade', 'deny'),
    ].join('\n');
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    expect((r.body as FrictionBody).totalFriction).toBe(1);
  });

  test('missing role lands in unknown', () => {
    const ts = new Date(FIXED_NOW - HOUR_MS).toISOString();
    const log = JSON.stringify({ decision: 'deny', module: 'accept_gate', timestamp: ts });
    const r = fetchHookFriction({ readLog: () => log, now: nowFn });
    const body = r.body as FrictionBody;
    expect(body.hooks[0].byRole).toEqual({ unknown: 1 });
  });
});
