// @test-type: unit — formats ISO times the pulse APIs send into Boston time; no services
// @domain: messages
// @card: #4566
import { formatBostonWhen } from '../src/sources/format';
import { toActorRow } from '../src/sources/pulse';

// Jeff's 12:42 screenshot (2026-10-16) read "since 2026-10-16 15:59:33": raw UTC from the API.
describe('#4566 every time v2 shows is Boston time', () => {
  const now = new Date('2026-10-16T16:42:00Z'); // 12:42 Boston

  test('a time today shows as Boston HH:MM', () => {
    expect(formatBostonWhen('2026-10-16T15:59:33Z', now)).toBe('11:59');
  });

  test('a time on another day carries the Boston weekday', () => {
    expect(formatBostonWhen('2026-10-15T17:11:12Z', now)).toBe('Thu 13:11');
  });

  test('NEGATIVE PROOF: the waiting detail never carries the raw UTC string', () => {
    const row = toActorRow({
      aboutPrincipal: 'principal-abby-normal', stateValue: 'waiting', stateSource: 'observed', activity: 'idle',
      waitingOn: { on: 'principal-wren', what: 'action', since: '2026-10-16T15:59:33Z' }, sessions: [],
    } as never);
    expect(row.waiting).not.toContain('2026-10-16');
    expect(row.waiting).not.toContain('15:59');
    expect(row.waiting).toMatch(/since (\w{3} )?11:59/);
  });
});
