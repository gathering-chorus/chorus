/**
 * #3125 — session registry resolver tests.
 *
 * The registry maps a role to its live session(s): {role, pid, tty, host},
 * written at SessionStart. Delivery resolves role → tty here instead of
 * letting chorus-inject guess by window title. AC1 (resolve), AC2 (dead
 * sessions never resolved).
 */
import { resolveTarget, planDelivery, type SessionReg } from './session-registry';

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
