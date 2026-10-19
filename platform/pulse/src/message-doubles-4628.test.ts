// @test-type: unit — pure count over fixture notes
// @domain: messages
// @card: #4628
// @owner: silas
/**
 * #4628 AC3/AC4 — doubles in Buzz, per Boston day and per kind. A double is
 * the #4646 rule the room already uses: same author, same words, within 20 s.
 * The words are compared without a role's "--- Role | time ---" header line,
 * because the role's own key posts a reply with it and Clearing v1 posted the
 * same reply without it (measured 10-19: ~530 such pairs a day, invisible to a
 * byte compare). Measured gaps that day: 454 of 465 doubles within 20 s.
 */
import { countDoubles, DOUBLE_KINDS } from './message-counts';

const at = (iso: string) => Date.parse(iso) / 1000;
const note = (iso: string, content: string, tags: string[][]) => ({ content, tags, created_at: at(iso) });
const v2Jeff = (iso: string, text: string) => note(iso, text, [['t', 'team'], ['room', 'team'], ['author', 'jeff'], ['signed', 'x']]);
const v1 = (iso: string, author: string, text: string) => note(iso, text, [['t', 'team'], ['author', author]]);
const roleReply = (iso: string, role: string, text: string) => note(iso, text, [['t', 'reply'], ['role', role], ['seq', '1'], ['hash', 'h']]);
const pulseNudge = (iso: string, from: string, text: string) => note(iso, text, [['t', 'team'], ['author', from], ['kind', 'nudge'], ['msg', 'pulse-1']]);
const v1Nudge = (iso: string, from: string, text: string) => note(iso, text, [['t', 'team'], ['author', from], ['kind', 'nudge']]);

const day = (rows: ReturnType<typeof countDoubles>, d: string) =>
  Object.fromEntries(rows.filter((r) => r.day === d).map((r) => [r.kind, r.doubles]));

describe('#4628 doubles in Buzz, per day and kind', () => {
  test('v2 then v1 copying the same Jeff message is one Jeff double', () => {
    const rows = countDoubles([
      v2Jeff('2026-10-18T20:41:46Z', 'ok router is in living room'),
      v1('2026-10-18T20:41:49Z', 'jeff', 'ok router is in living room'),
    ], '2026-10-19');
    expect(day(rows, '2026-10-18').jeff).toBe(1);
  });

  test('a role reply posted with its header and again without it is one reply double', () => {
    const rows = countDoubles([
      roleReply('2026-10-18T20:53:21Z', 'silas', '--- Silas | 2026-10-18 16:53 Boston | #4628 | Werk v1.8.1 ---\n\nThe pair on #4628 has started.'),
      v1('2026-10-18T20:53:20Z', 'silas', 'The pair on #4628 has started.'),
    ], '2026-10-19');
    expect(day(rows, '2026-10-18').reply).toBe(1);
  });

  test('two writers stamping the same second are still a double (35 of 10-19\'s pairs; a strict "earlier" check misses them)', () => {
    const rows = countDoubles([
      v1('2026-10-19T16:42:53Z', 'wren', 'Silas\'s plan for after #4634 lands'),
      roleReply('2026-10-19T16:42:53Z', 'wren', '--- Wren | 2026-10-19 12:45 Boston | — | Werk v1.8.1 ---\n\nSilas\'s plan for after #4634 lands'),
    ], '2026-10-20');
    expect(day(rows, '2026-10-19').reply).toBe(1);
  });

  test('every kind gets a row for a day with notes, 0 when it had no doubles', () => {
    const rows = countDoubles([v1('2026-10-18T12:00:00Z', 'wren', 'hello')], '2026-10-19');
    expect(day(rows, '2026-10-18')).toEqual(Object.fromEntries(DOUBLE_KINDS.map((k) => [k, 0])));
  });

  test('the same words from two authors, or 2 minutes apart, are not doubles; today waits for tomorrow', () => {
    const rows = countDoubles([
      v1('2026-10-18T12:00:00Z', 'wren', 'agreed'),
      v1('2026-10-18T12:00:05Z', 'kade', 'agreed'),
      v1('2026-10-18T12:02:05Z', 'kade', 'agreed'),
      v1('2026-10-19T12:00:00Z', 'jeff', 'today'),
      v1('2026-10-19T12:00:01Z', 'jeff', 'today'),
    ], '2026-10-19');
    expect(day(rows, '2026-10-18').reply).toBe(0);
    expect(rows.some((r) => r.day === '2026-10-19')).toBe(false);
  });

  test('land notices and alerts are their own kinds', () => {
    const rows = countDoubles([
      v1('2026-10-18T20:49:07Z', 'unattributed', 'Accepted #4630 — Backups'),
      v1('2026-10-18T20:49:09Z', 'unattributed', 'Accepted #4630 — Backups'),
      note('2026-10-18T13:00:00Z', '[alert → silas] log stale', [['t', 'team'], ['author', 'system'], ['kind', 'alert'], ['msg', 'pulse-2']]),
      note('2026-10-18T13:00:02Z', '[alert → silas] log stale', [['t', 'team'], ['author', 'system'], ['kind', 'alert']]),
    ], '2026-10-19');
    expect(day(rows, '2026-10-18')).toMatchObject({ land: 1, alert: 1 });
  });

  // NEGATIVE PROOF (AC4): a second writer left in place shows as doubles; one writer shows none.
  test('negative proof: two writers for one nudge read 1 double, one writer reads 0', () => {
    const one = countDoubles([pulseNudge('2026-10-18T12:00:00Z', 'kade', '[nudge from kade → wren] ping')], '2026-10-19');
    const two = countDoubles([
      pulseNudge('2026-10-18T12:00:00Z', 'kade', '[nudge from kade → wren] ping'),
      v1Nudge('2026-10-18T12:00:02Z', 'kade', '[nudge from kade → wren] ping'),
    ], '2026-10-19');
    expect(day(one, '2026-10-18').nudge).toBe(0);
    expect(day(two, '2026-10-18').nudge).toBe(1);
  });
});
