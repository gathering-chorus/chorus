// @test-type: unit — #3700 (Silas half): the typed delivery decision. Replaces
// the null→name-match fallthrough with four explicit outcomes. Jeff's 07-04
// visibility ruling is honored by TYPE, not by wrong-terminal keystroke: an
// undeliverable nudge reports its reason to the SENDER + spine — never silent,
// never sprayed into another role's terminal (the 2026-07-26 incident).
import { planDeliveryTyped, type SessionReg, type TurnState } from './session-registry';

const reg = (over: Partial<SessionReg>): SessionReg => ({
  role: 'wren', pid: 200, tty: '/dev/ttys004', host: 'tmux', tmux: '%0',
  registered_at: '2026-07-26T13:00:00Z', ...over,
});
const idle: TurnState = { busy: false };
const busy: TurnState = { busy: true, since: '2026-07-26T13:50:00Z' };

describe('#3700 planDeliveryTyped — the four outcomes', () => {
  test('idle live target → inject NOW (nudge-as-wake preserved, transport args from planDelivery)', () => {
    const p = planDeliveryTyped([reg({})], 'wren', 'hello', () => true, () => idle);
    expect(p.kind).toBe('inject');
    expect(p.kind === 'inject' && p.args).toEqual(['--tmux', '%0', 'hello']);
  });

  test('busy live target → queue (typed; drained at the target turn boundary, no spray)', () => {
    // clock PINNED — an unpinned Date.now let this test rot past the 30-min
    // staleness bound 70 minutes after it was written (caught 2026-07-26).
    const p = planDeliveryTyped([reg({})], 'wren', 'hello', () => true, () => busy,
      () => Date.parse('2026-07-26T14:00:00Z'));
    expect(p.kind).toBe('queue');
    expect(p.kind === 'queue' && p.reason).toContain('busy');
  });

  test('dead registration → undelivered(dead) — typed, never name-match', () => {
    const p = planDeliveryTyped([reg({ pid: 999 })], 'wren', 'hello', (pid) => pid !== 999, () => idle);
    expect(p.kind).toBe('undelivered');
    expect(p.kind === 'undelivered' && p.reason).toBe('dead');
  });

  test('no registration → undelivered(unregistered) — typed, never name-match', () => {
    const p = planDeliveryTyped([], 'wren', 'hello', () => true, () => idle);
    expect(p.kind).toBe('undelivered');
    expect(p.kind === 'undelivered' && p.reason).toBe('unregistered');
  });

  test('busy but STALE turn marker (>30min) decays to idle → inject (crash guard)', () => {
    const stale: TurnState = { busy: true, since: '2026-07-26T13:00:00Z' }; // 60min before now
    const p = planDeliveryTyped([reg({})], 'wren', 'hello', () => true, () => stale,
      () => Date.parse('2026-07-26T14:00:00Z'));
    expect(p.kind).toBe('inject');
  });
});
