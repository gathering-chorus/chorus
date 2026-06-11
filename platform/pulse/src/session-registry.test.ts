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

// #3352 AC-0 — sender-aware planDelivery: the misdelivery class is refused
// structurally (defer to fold), never keystroked into the sender's session.
describe('#3352 planDelivery sender-collision refuse', () => {
  const reg = (role: string, pid: number, tty: string | undefined, host: string) =>
    ({ role, pid, tty, host, registered_at: '1781199536' }) as never;

  test('target sharing the SENDER pid defers (the 2026-06-11 silas/wren collision)', () => {
    const silas = reg('silas', 62547, '/dev/ttys003', 'vscode');
    const wren = reg('wren', 62547, '/dev/ttys003', 'vscode');
    const plan = planDelivery(silas, 'silas', 'gather nudge', wren);
    expect(plan).toEqual({ kind: 'defer', reason: 'sender-collision' });
  });

  test('target sharing the SENDER tty (different pid) also defers', () => {
    const target = reg('silas', 1111, '/dev/ttys003', 'terminal');
    const sender = reg('wren', 2222, '/dev/ttys003', 'terminal');
    expect(planDelivery(target, 'silas', 'x', sender).kind).toBe('defer');
  });

  test('vscode target with vscode sender defers (focus cannot address a session)', () => {
    const target = reg('silas', 1111, '/dev/ttys004', 'vscode');
    const sender = reg('wren', 2222, '/dev/ttys003', 'vscode');
    expect(planDelivery(target, 'silas', 'x', sender)).toEqual({ kind: 'defer', reason: 'vscode-ambiguous-with-sender' });
  });

  test('distinct terminal sessions inject by tty as before (no regression)', () => {
    const target = reg('silas', 1111, '/dev/ttys000', 'terminal');
    const sender = reg('wren', 2222, '/dev/ttys003', 'vscode');
    expect(planDelivery(target, 'silas', 'hello', sender)).toEqual({ kind: 'inject', args: ['--tty', '/dev/ttys000', 'hello'] });
  });

  test('no sender registration (jeff-input, system) keeps pre-#3352 behavior', () => {
    const target = reg('silas', 1111, undefined, 'vscode');
    expect(planDelivery(target, 'silas', 'x', null)).toEqual({ kind: 'inject', args: ['--vscode', 'x'] });
    expect(planDelivery(null, 'silas', 'x', null)).toEqual({ kind: 'inject', args: ['silas', 'x'] });
  });
});
