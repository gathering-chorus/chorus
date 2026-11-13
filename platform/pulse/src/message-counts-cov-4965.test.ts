// @domain: awareness
// @test-type: unit — sentFromPulse, countKinds, countDays and noteKind in message-counts.ts against in-memory rows; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — what the messages count says per kind: a nudge to Jeff, a system
 * alert and a role nudge each carry the text the room shows; an empty nudge is
 * not a sent message; a reply-tagged note counts as a role reply, never as a
 * match for a sent message; a note with no time belongs to no day.
 */
import { sentFromPulse, countKinds, countDays, noteKind, bostonMidnight, bostonDay } from './message-counts';

describe('#4965 message-counts names each sent kind by what the room carries', () => {
  test('a nudge to Jeff, a system alert and a role nudge each read as their own kind and text', () => {
    const sent = sentFromPulse([
      { type: 'nudge', from: 'kade', to: 'jeff', content: '[nudge from kade | 10:00] look' },
      { type: 'nudge', from: 'system', to: 'wren', content: 'run failed' },
      { type: 'nudge', from: 'silas', to: 'kade', content: 'pair?' },
    ]);
    expect(sent).toEqual([
      { kind: 'nudge-to-jeff', text: 'look' },
      { kind: 'system-alert', text: '[alert → wren] run failed' },
      { kind: 'nudge-between-roles', text: '[nudge from silas → kade] pair?' },
    ]);
  });

  test('negative: a nudge whose words are empty after the header is not a sent message', () => {
    expect(sentFromPulse([{ type: 'nudge', from: 'kade', to: 'wren', content: '[nudge from kade | 10:00] ' }])).toEqual([]);
  });

  test('a reply-tagged note counts as a role reply and reaches no sent message of the same text', () => {
    const sent = [{ kind: 'nudge-to-jeff' as const, text: 'same words' }];
    const notes = [{ content: 'same words', tags: [['t', 'reply']] }];
    const kinds = countKinds(sent, notes);
    expect(kinds.find((k) => k.kind === 'role-reply')).toEqual({ kind: 'role-reply', label: 'Role reply', sent: null, reached: 1, gap: null });
    // negative: the reply did not satisfy the nudge with the same text — the gap is 1, not 0
    expect(kinds.find((k) => k.kind === 'nudge-to-jeff')).toMatchObject({ sent: 1, reached: 0, gap: 1 });
  });

  test('negative: a note with no numeric time is kept on no day', () => {
    const rows = [{ type: 'nudge', from: 'kade', to: 'jeff', content: 'hello', created_at: '2026-10-17 16:00:00' }];
    const dayWithTimed = countDays(rows, [{ content: 'hello', tags: [], created_at: Math.floor(Date.parse('2026-10-17T16:00:05Z') / 1000) }], '2026-10-18');
    const dayWithUntimed = countDays(rows, [{ content: 'hello', tags: [] }], '2026-10-18');
    expect(dayWithTimed).toEqual([{ day: '2026-10-17', sent: 1, kept: 1 }]);
    expect(dayWithUntimed).toEqual([{ day: '2026-10-17', sent: 1, kept: 0 }]);
  });

  test('a note authored by a role with no kind tag reads as a reply; an unattributed one as a land', () => {
    expect(noteKind({ content: 'x', tags: [['author', 'silas']] })).toBe('reply');
    expect(noteKind({ content: 'x', tags: [['author', 'unattributed']] })).toBe('land');
  });

  test('Boston midnight is the start of the Boston day it falls in', () => {
    const now = new Date('2026-11-13T15:00:00Z');
    const midnight = bostonMidnight(now);
    expect(bostonDay(midnight * 1000)).toBe('2026-11-13');
    expect(bostonDay(midnight * 1000 - 1000)).toBe('2026-11-12');
  });
});
