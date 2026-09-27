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
import { planDelivery, describeTarget, type SessionReg } from './session-registry';

const reg = (over: Partial<SessionReg>): SessionReg => ({
  role: 'silas', pid: 100, tty: '/dev/ttys001', host: 'terminal',
  registered_at: '2026-05-29T13:00:00Z', ...over,
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

  // #3668 — a registration carrying a tmux pane routes the app-level tmux
  // transport (locked-screen-safe), beating both the vscode keystroke path
  // and the Terminal --tty path: the tmux server is the true input surface
  // wherever the session is displayed.
  test('#3668 tmux pane on the registration → --tmux exact pane', () => {
    const t = reg({ role: 'wren', tty: '/dev/ttys006', host: 'tmux', tmux: '%3' });
    expect(planDelivery(t, 'wren', 'go')).toEqual({ kind: 'inject', args: ['--tmux', '%3', 'go'] });
  });

  test('#3668 tmux pane wins even if host still says vscode (belt + suspenders)', () => {
    const t = reg({ role: 'wren', tty: '/dev/ttys006', host: 'vscode', tmux: '%12' });
    expect(planDelivery(t, 'wren', 'go')).toEqual({ kind: 'inject', args: ['--tmux', '%12', 'go'] });
  });

  test('#3668 vscode WITHOUT a pane keeps the keystroke path (rollout fallback)', () => {
    const t = reg({ role: 'wren', tty: '/dev/ttys004', host: 'vscode' });
    expect(planDelivery(t, 'wren', 'hello')).toEqual({ kind: 'inject', args: ['--vscode', 'hello'] });
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


// #4361 regression (live 2026-09-27 11:59): Presence targets carry no pid
// (pid 0). The #3352 same-session rule compared pids, so EVERY sender matched
// every target and delivery fell to name-match → chorus-inject no-window-found.
describe('#4361 same-session rule with Presence targets (pid 0)', () => {
  const silas: SessionReg = { role: 'silas', pid: 0, tty: '/dev/ttys005', host: 'tmux', tmux: '%15' };
  const wren: SessionReg = { role: 'wren', pid: 0, tty: '/dev/ttys006', host: 'tmux', tmux: '%14' };
  test('two different panes with pid 0 are NOT the same session → tmux delivery', () => {
    expect(planDelivery(silas, 'silas', 'hi', wren)).toEqual({ kind: 'inject', args: ['--tmux', '%15', 'hi'] });
  });
  test('the same pane IS the same session → name-match (the #3352 rule still holds)', () => {
    expect(planDelivery(silas, 'silas', 'hi', { ...wren, tmux: '%15', tty: '/dev/ttys005' }).kind).toBe('inject');
    expect(planDelivery(silas, 'silas', 'hi', { ...wren, tmux: '%15', tty: '/dev/ttys005' })).toEqual({ kind: 'inject', args: ['silas', 'hi'] });
  });
});
