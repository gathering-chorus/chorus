// @test-type: unit — pure count over fixture rows and notes
/**
 * #4634 — the countKinds rule, now in pulse, counted per Boston day: what
 * should have reached Buzz (sent) and what did (kept).
 */
import { countDays, countKinds, sentFromPulse, bostonDay } from './message-counts';

const row = (created_at: string, from: string, to: string, content: string, type = 'nudge') => ({ created_at, type, from, to, content });

describe('#4634 messages kept in Buzz, per Boston day', () => {
  test('a day where every nudge reached the room keeps them all; a day where none did keeps 0', () => {
    const rows = [
      row('2026-10-16 14:00:00', 'kade', 'silas', '[nudge from kade | 2026-10-16 10:00 Boston] hi'),
      row('2026-10-17 14:00:00', 'kade', 'silas', '[nudge from kade | 2026-10-17 10:00 Boston] again'),
    ];
    const notes = [{ content: '[nudge from kade → silas] hi', tags: [['t', 'team']], created_at: Date.parse('2026-10-16T14:00:01Z') / 1000 }];
    expect(countDays(rows, notes, '2026-10-18')).toEqual([
      { day: '2026-10-16', sent: 1, kept: 1 },
      { day: '2026-10-17', sent: 1, kept: 0 },
    ]);
  });

  test('today is left to the next pass, and a UTC-morning message lands on the Boston day before', () => {
    const rows = [
      row('2026-10-18 03:30:00', 'kade', 'silas', 'late'),
      row('2026-10-18 16:00:00', 'kade', 'silas', 'today'),
    ];
    expect(countDays(rows, [], '2026-10-18')).toEqual([{ day: '2026-10-17', sent: 1, kept: 0 }]);
    expect(bostonDay(Date.parse('2026-10-18T03:30:00Z'))).toBe('2026-10-17');
  });

  test('negative proof: a note with different words does not count as kept', () => {
    const sent = sentFromPulse([row('2026-10-17 14:00:00', 'kade', 'silas', 'the real words')]);
    const kinds = countKinds(sent, [{ content: '[nudge from kade → silas] other words', tags: [['t', 'team']] }]);
    expect(kinds.find((k) => k.kind === 'nudge-between-roles')).toMatchObject({ sent: 1, reached: 0, gap: 1 });
  });
});
