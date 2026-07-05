// @test-type: integration — #3608 sweep tests touch a real tmpdir (mkdtemp); resolver tests are pure
/* eslint-disable sonarjs/no-duplicate-string -- repeated fixture literals (session ids/paths) are intentional for per-case readability (#3429) */
/**
 * #3125 — session registry resolver tests.
 *
 * The registry maps a role to its live session(s): {role, pid, tty, host},
 * written at SessionStart. Delivery resolves role → tty here instead of
 * letting chorus-inject guess by window title. AC1 (resolve), AC2 (dead
 * sessions never resolved).
 */
import { mkdtempSync, writeFileSync, readdirSync, rmSync } from 'fs';
import os from 'os';
import path from 'path';
import { resolveTarget, planDelivery, describeTarget, sweepRegistry, resolveRoleTarget, type SessionReg } from './session-registry';

const reg = (over: Partial<SessionReg>): SessionReg => ({
  role: 'silas', pid: 100, tty: '/dev/ttys001', host: 'terminal',
  registered_at: '2026-05-29T13:00:00Z', ...over,
});

describe('resolveTarget', () => {
  test('resolves a role to its live session tty (AC1)', () => {
    const regs = [reg({ role: 'silas', pid: 100, tty: '/dev/ttys001' })];
    const out = resolveTarget(regs, 'silas', () => true);
    expect(out?.tty).toBe('/dev/ttys001');
    expect(out?.host).toBe('terminal');
  });

  test('never resolves a dead session (AC2 — liveness filter)', () => {
    const regs = [reg({ role: 'silas', pid: 999, tty: '/dev/ttys001' })];
    const out = resolveTarget(regs, 'silas', (pid) => pid !== 999); // 999 is dead
    expect(out).toBeNull();
  });

  test('does not bleed across roles (AC5 — no cross-targeting)', () => {
    const regs = [
      reg({ role: 'wren', pid: 200, tty: '/dev/ttys004', host: 'vscode' }),
      reg({ role: 'silas', pid: 100, tty: '/dev/ttys001', host: 'terminal' }),
    ];
    expect(resolveTarget(regs, 'silas', () => true)?.tty).toBe('/dev/ttys001');
    expect(resolveTarget(regs, 'wren', () => true)?.tty).toBe('/dev/ttys004');
  });

  test('two live sessions for one role: picks most-recently-registered', () => {
    const regs = [
      reg({ role: 'wren', pid: 200, tty: '/dev/ttys004', registered_at: '2026-05-29T10:00:00Z' }),
      reg({ role: 'wren', pid: 201, tty: '/dev/ttys007', registered_at: '2026-05-29T13:00:00Z' }),
    ];
    expect(resolveTarget(regs, 'wren', () => true)?.tty).toBe('/dev/ttys007');
  });

  test('returns null when no session registered (→ caller falls back to name-match, as-is)', () => {
    expect(resolveTarget([], 'kade', () => true)).toBeNull();
  });

  test('surfaces vscode host so caller can choose inbox fallback (AC6)', () => {
    const regs = [reg({ role: 'wren', pid: 200, tty: '/dev/ttys004', host: 'vscode' })];
    expect(resolveTarget(regs, 'wren', () => true)?.host).toBe('vscode');
  });
});

describe('planDelivery', () => {
  test('terminal host with tty → --tty exact match (AC3 live)', () => {
    const t = reg({ role: 'silas', tty: '/dev/ttys001', host: 'terminal' });
    expect(planDelivery(t, 'silas', 'hello')).toEqual({ kind: 'inject', args: ['--tty', '/dev/ttys001', 'hello'] });
  });

  test('#3130 vscode host → --vscode inject (Code app, not the Terminal --tty path that no-window-founds)', () => {
    const t = reg({ role: 'wren', tty: '/dev/ttys004', host: 'vscode' });
    // A VS Code pseudo-tty is not a Terminal tab, so --tty returns no-window-found.
    // Route vscode to the Code-app focused-window inject instead.
    expect(planDelivery(t, 'wren', 'hello')).toEqual({ kind: 'inject', args: ['--vscode', 'hello'] });
  });

  test('no registration → legacy name-match (as-is preserved)', () => {
    expect(planDelivery(null, 'kade', 'hello')).toEqual({ kind: 'inject', args: ['kade', 'hello'] });
  });
});

// #3352 final form (Jeff, DEC-107 re-affirmed): delivery is UNCONDITIONAL —
// every plan is an inject (osascript all the time). A target registration that
// collides with the SENDER is stale data: ignored, falls to role name-match —
// still a keystroke, never a skip.
describe('#3352 planDelivery always injects', () => {
  const reg = (role: string, pid: number, tty: string | undefined, host: string) =>
    ({ role, pid, tty, host, registered_at: '1781199536' }) as never;

  test('target sharing the SENDER pid (stale reg) falls to name-match — still injects', () => {
    const silas = reg('silas', 62547, '/dev/ttys003', 'vscode');
    const wren = reg('wren', 62547, '/dev/ttys003', 'vscode');
    // #3608: Jeff KEPT this unconditional (2026-07-04) — a visible misdelivery
    // is the alarm. Poison prevention lives upstream (env-verify + sweep).
    expect(planDelivery(silas, 'silas', 'gather nudge', wren)).toEqual({ kind: 'inject', args: ['silas', 'gather nudge'] });
  });

  test('vscode target injects --vscode even when sender is also vscode', () => {
    const target = reg('silas', 1111, '/dev/ttys004', 'vscode');
    const sender = reg('wren', 2222, '/dev/ttys003', 'vscode');
    expect(planDelivery(target, 'silas', 'x', sender)).toEqual({ kind: 'inject', args: ['--vscode', 'x'] });
  });

  test('distinct terminal sessions inject by tty', () => {
    const target = reg('silas', 1111, '/dev/ttys000', 'terminal');
    const sender = reg('wren', 2222, '/dev/ttys003', 'vscode');
    expect(planDelivery(target, 'silas', 'hello', sender)).toEqual({ kind: 'inject', args: ['--tty', '/dev/ttys000', 'hello'] });
  });

  test('no registration anywhere → legacy name-match inject', () => {
    expect(planDelivery(null, 'silas', 'x', null)).toEqual({ kind: 'inject', args: ['silas', 'x'] });
  });

  test('NO plan shape is ever a skip: every case above returned kind inject', () => {
    // the contract Jeff locked: osascript all the time — defer is not a delivery outcome
    const shapes = [
      planDelivery(reg('silas', 1, '/dev/ttys001', 'terminal'), 'silas', 'a', reg('wren', 1, '/dev/ttys001', 'terminal')),
      planDelivery(reg('silas', 2, undefined, 'vscode'), 'silas', 'b', reg('wren', 3, '/dev/ttys003', 'vscode')),
      planDelivery(null, 'kade', 'c', null),
    ];
    for (const p of shapes) expect(p.kind).toBe('inject');
  });
});

// #3439 AC3 — the MCP must report WHERE a nudge resolved, not a blind "sent".
// describeTarget is the pure formatter the pulse POST surfaces in its response.
describe('#3439 describeTarget — report resolved destination (AC3)', () => {
  test('live session → "role @ tty (host, pid)"', () => {
    const t = reg({ role: 'kade', pid: 321, tty: '/dev/ttys003', host: 'terminal' });
    expect(describeTarget('kade', t)).toBe('kade @ /dev/ttys003 (terminal, pid 321)');
  });

  test('no live session → explicit name-match fallback (not a silent blind "sent")', () => {
    expect(describeTarget('kade', null)).toBe('kade [no live session — name-match fallback]');
  });

  test('vscode host is surfaced so a mis-route shows in the report', () => {
    const t = reg({ role: 'wren', pid: 200, tty: '/dev/ttys004', host: 'vscode' });
    expect(describeTarget('wren', t)).toContain('(vscode, pid 200)');
  });
});


// ── #3608 — role re-verification + registry self-heal ──────────────────────

describe('#3608 resolveTarget role re-verification', () => {
  test('excludes a poisoned entry: registration says silas but the pid runs wren', () => {
    const regs = [reg({ role: 'silas', pid: 74581, tty: '/dev/ttys001', host: 'vscode' })];
    const roleOf = (pid: number) => (pid === 74581 ? 'wren' : null);
    expect(resolveTarget(regs, 'silas', () => true, roleOf)).toBeNull();
  });

  test('prefers the verified entry over a newer poisoned one', () => {
    const regs = [
      reg({ role: 'silas', pid: 81082, tty: '/dev/ttys002', registered_at: '100' }),
      reg({ role: 'silas', pid: 74581, tty: '/dev/ttys001', registered_at: '999' }), // newer but poisoned
    ];
    const roleOf = (pid: number) => (pid === 74581 ? 'wren' : 'silas');
    expect(resolveTarget(regs, 'silas', () => true, roleOf)?.tty).toBe('/dev/ttys002');
  });

  test('unverifiable (null) keeps the entry — never strand on uncertainty', () => {
    const regs = [reg({ role: 'silas', pid: 100, tty: '/dev/ttys002' })];
    expect(resolveTarget(regs, 'silas', () => true, () => null)?.tty).toBe('/dev/ttys002');
  });
});

describe('#3608 sweepRegistry self-heal', () => {
  let dir: string;
  beforeEach(() => { dir = mkdtempSync(path.join(os.tmpdir(), 'reg-sweep-')); });
  afterEach(() => { rmSync(dir, { recursive: true, force: true }); });

  const writeReg = (r: SessionReg) =>
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- test-controlled tmpdir fixture path, never caller input (#3606)
    writeFileSync(path.join(dir, `${r.role}-${r.pid}.json`), JSON.stringify(r));

  test('deletes poisoned + dead entries, keeps verified live ones, emits spine events (AC2)', () => {
    writeReg(reg({ role: 'silas', pid: 74581, tty: '/dev/ttys001' })); // poisoned (pid runs wren)
    writeReg(reg({ role: 'silas', pid: 999, tty: '/dev/ttys009' }));   // dead
    writeReg(reg({ role: 'wren', pid: 74581, tty: '/dev/ttys001' }));  // verified live
    const events: Array<{ event: string; fields: Record<string, string> }> = [];
    const swept = sweepRegistry(dir, (pid) => pid !== 999, (pid) => (pid === 74581 ? 'wren' : null),
      (event, fields) => events.push({ event, fields }));
    expect(swept).toHaveLength(2);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- test-controlled tmpdir fixture path, never caller input (#3606)
    expect(readdirSync(dir)).toEqual(['wren-74581.json']);
    expect(events.map((e) => e.event).sort()).toEqual(['routing.poison.detected', 'routing.stale.swept']);
    const poison = events.find((e) => e.event === 'routing.poison.detected');
    expect(poison?.fields).toMatchObject({ reg_role: 'silas', pid: '74581', actual_role: 'wren' });
  });

  test('resolveRoleTarget self-heals then resolves: poison gone, correct target returned', () => {
    writeReg(reg({ role: 'silas', pid: 74581, tty: '/dev/ttys001', registered_at: '999' })); // poison at wren pid
    writeReg(reg({ role: 'silas', pid: 81082, tty: '/dev/ttys002', registered_at: '100' })); // real silas
    const roleOf = (pid: number) => (pid === 74581 ? 'wren' : pid === 81082 ? 'silas' : null);
    const out = resolveRoleTarget('silas', dir, () => true, roleOf, () => {});
    expect(out?.tty).toBe('/dev/ttys002');
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- test-controlled tmpdir fixture path, never caller input (#3606)
    expect(readdirSync(dir).sort()).toEqual(['silas-81082.json']);
  });
});
