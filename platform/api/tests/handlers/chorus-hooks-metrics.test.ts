/**
 * chorus-hooks-metrics handler — unit tests (#2188).
 */
import { fetchChorusHooksMetrics } from '../../src/handlers/chorus-hooks-metrics';

const FIXED_NOW = new Date('2026-04-18T12:00:00Z').getTime();
const nowFn = () => FIXED_NOW;
const DAY_MS = 24 * 60 * 60 * 1000;

function line(
  daysAgo: number, hookType: string, tool: string, role: string, module: string, decision: string,
): string {
  const ts = new Date(FIXED_NOW - daysAgo * DAY_MS).toISOString();
  return `${ts} | ${hookType} | ${tool} | ${role} | ${module} | ${decision} | 5ms | session123 | ctx`;
}

describe('fetchChorusHooksMetrics (#2188)', () => {
  test('readLog null → 503', () => {
    const r = fetchChorusHooksMetrics({ readLog: () => null, now: nowFn });
    expect(r.status).toBe(503);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/hooks\.log/);
  });

  test('empty log → 200 zero counts', () => {
    const r = fetchChorusHooksMetrics({ readLog: () => '', now: nowFn });
    expect(r.status).toBe(200);
    const body = r.body as { totalDecisions: number; totalModules: number; enforcementPercent: number };
    expect(body.totalDecisions).toBe(0);
    expect(body.totalModules).toBe(0);
    expect(body.enforcementPercent).toBe(0);
  });

  test('rows older than 7 days are excluded', () => {
    const log = [
      line(10, 'pre-tool', 'bash', 'wren', 'old_mod', 'allow'),
      line(1, 'pre-tool', 'bash', 'wren', 'new_mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { modules: Record<string, unknown> };
    expect(Object.keys(body.modules)).toEqual(['new_mod']);
  });

  test('rows with empty/dash/none module are skipped', () => {
    const log = [
      line(1, 'pre', 'bash', 'wren', '', 'allow'),
      line(1, 'pre', 'bash', 'wren', '-', 'allow'),
      line(1, 'pre', 'bash', 'wren', 'none', 'allow'),
      line(1, 'pre', 'bash', 'wren', 'real_mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { modules: Record<string, unknown> };
    expect(Object.keys(body.modules)).toEqual(['real_mod']);
  });

  test('decision=enter is skipped', () => {
    const log = [
      line(1, 'pre', 'bash', 'wren', 'mod', 'enter'),
      line(1, 'pre', 'bash', 'wren', 'mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { modules: Record<string, { allow: number; total: number }> };
    expect(body.modules.mod).toEqual({ allow: 1, deny: 0, warn: 0, total: 1 });
  });

  test('deny and block both increment deny counter', () => {
    const log = [
      line(1, 'pre', 'bash', 'wren', 'mod', 'deny'),
      line(1, 'pre', 'bash', 'wren', 'mod', 'block'),
      line(1, 'pre', 'bash', 'wren', 'mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { modules: Record<string, { deny: number; allow: number; total: number }> };
    expect(body.modules.mod).toMatchObject({ deny: 2, allow: 1, total: 3 });
  });

  test('warn increments warn counter', () => {
    const log = [
      line(1, 'pre', 'bash', 'wren', 'mod', 'WARN'),
      line(1, 'pre', 'bash', 'wren', 'mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { modules: Record<string, { warn: number; total: number }> };
    expect(body.modules.mod.warn).toBe(1);
    expect(body.modules.mod.total).toBe(2);
  });

  test('enforcementPercent = modules with any deny / total modules', () => {
    const log = [
      line(1, 'pre', 'bash', 'wren', 'enforcing', 'deny'),
      line(1, 'pre', 'bash', 'wren', 'enforcing', 'allow'),
      line(1, 'pre', 'bash', 'wren', 'allow_only', 'allow'),
      line(1, 'pre', 'bash', 'wren', 'warn_only', 'warn'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as {
      enforcedModules: number; totalModules: number; enforcementPercent: number;
    };
    expect(body.enforcedModules).toBe(1);
    expect(body.totalModules).toBe(3);
    expect(body.enforcementPercent).toBe(33);
  });

  test('rows with fewer than 6 pipe-parts are skipped', () => {
    const log = [
      'malformed | line | too | short',
      line(1, 'pre', 'bash', 'wren', 'mod', 'allow'),
    ].join('\n');
    const body = fetchChorusHooksMetrics({ readLog: () => log, now: nowFn }).body as { totalDecisions: number };
    expect(body.totalDecisions).toBe(1);
  });

  test('generatedAt uses injected now()', () => {
    const body = fetchChorusHooksMetrics({ readLog: () => '', now: nowFn }).body as { generatedAt: string };
    expect(body.generatedAt).toBe('2026-04-18T12:00:00.000Z');
  });

  test('periodDays is always 7', () => {
    const body = fetchChorusHooksMetrics({ readLog: () => '', now: nowFn }).body as { periodDays: number };
    expect(body.periodDays).toBe(7);
  });
});
