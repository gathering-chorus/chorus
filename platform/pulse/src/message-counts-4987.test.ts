// @test-type: unit — the messages sensor's row-time parse and day count over fixture rows; no store, no relay.
// @domain: messages
// @card: #4987
// @owner: wren
// #4987 (AC5): the messages sensor reads SQL and ISO row times
/**
 * #4987 (folded from #4984, Jeff 06:33: "lets do timestamp fix on this card") — Silas measured
 * 11-17 04:47: the daily messages reading signed in and read every page, then failed with
 * "Invalid time value". 4 messages.db rows (BQ nudges, 11-07 00:53Z) are ISO, the rest SQL;
 * the reader appended a "Z" to every row, so the ISO ones became "…ZZ" and NaN.
 */
import { pulseRowTime, countDays, type DatedPulseRow } from './message-counts';

test('#4987 AC5 a SQL row and an ISO row both read as the same instant they name', () => {
  expect(pulseRowTime('2026-11-07 00:53:24')).toBe(Date.UTC(2026, 10, 7, 0, 53, 24));
  expect(pulseRowTime('2026-11-07T00:53:24.406Z')).toBe(Date.UTC(2026, 10, 7, 0, 53, 24, 406));
  expect(pulseRowTime('2026-11-07T00:53:24-05:00')).toBe(Date.UTC(2026, 10, 7, 5, 53, 24));
});

test('#4987 AC5 the day count takes the ISO rows with the rest (they broke the whole reading)', () => {
  const row = (created_at: string): DatedPulseRow => ({ created_at, type: 'nudge', from: 'borg-queen', to: 'wren', content: `n ${created_at}` });
  const days = countDays([row('2026-11-07 15:00:00'), row('2026-11-07T16:00:00.000Z')], [], '2026-11-08');
  expect(days).toEqual([{ day: '2026-11-07', sent: 2, kept: 0 }]);
});

test('#4987 AC5 NEGATIVE PROOF: the old append-a-Z parse fails the ISO row; anything not SQL or zoned ISO throws, naming it', () => {
  expect(Number.isNaN(Date.parse('2026-11-07T00:53:24.406Z'.replace(' ', 'T') + 'Z'))).toBe(true);
  expect(() => pulseRowTime('2026-11-07T00:53:24')).toThrow(/neither SQL nor ISO with a zone: 2026-11-07T00:53:24/);
  expect(() => pulseRowTime('yesterday')).toThrow(/yesterday/);
});
