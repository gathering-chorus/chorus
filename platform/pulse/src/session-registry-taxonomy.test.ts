// @test-type: unit — #3700 (Silas half): typed resolution taxonomy. A resolve
// miss must SAY WHY (dead vs unregistered) so delivery can refuse with a typed
// outcome instead of falling through to legacy name-match — the fallthrough
// that sprayed wren's probes into kade+silas terminals on 2026-07-26.
import { resolveTargetTyped, type SessionReg } from './session-registry';

const reg = (over: Partial<SessionReg>): SessionReg => ({
  role: 'wren', pid: 200, tty: '/dev/ttys004', host: 'tmux',
  registered_at: '2026-07-26T13:00:00Z', ...over,
});

describe('#3700 resolveTargetTyped — the fallback taxonomy', () => {
  test('live registered role resolves (kind=resolved, carries the session)', () => {
    const out = resolveTargetTyped([reg({})], 'wren', () => true);
    expect(out.kind).toBe('resolved');
    expect(out.kind === 'resolved' && out.session.tty).toBe('/dev/ttys004');
  });

  test('registered but dead pid → kind=dead (typed, NOT a bare null)', () => {
    const out = resolveTargetTyped([reg({ pid: 999 })], 'wren', (pid) => pid !== 999);
    expect(out.kind).toBe('dead');
  });

  test('no entry for the role at all → kind=unregistered', () => {
    const out = resolveTargetTyped([reg({ role: 'kade' })], 'wren', () => true);
    expect(out.kind).toBe('unregistered');
  });

  test('dead beats unregistered when both could describe the miss (an entry EXISTS)', () => {
    // one dead entry for the role + live entries for others → the truthful
    // answer is dead (the role WAS here), not unregistered.
    const regs = [reg({ pid: 999 }), reg({ role: 'kade', pid: 100, tty: '/dev/ttys003' })];
    expect(resolveTargetTyped(regs, 'wren', (pid) => pid !== 999).kind).toBe('dead');
  });

  test('poisoned entry (pid actual role differs) is not resolved — miss stays typed', () => {
    // registration says wren but the pid is actually kade's → never deliver
    // wren traffic there; the miss reports dead (poisoned = not a live wren).
    const out = resolveTargetTyped([reg({})], 'wren', () => true, () => 'kade');
    expect(out.kind).toBe('dead');
  });
});
