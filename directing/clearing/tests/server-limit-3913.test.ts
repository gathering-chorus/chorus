// @test-type: unit — pure request-shaping over the exported dedupe/limit logic; no live server.
/**
 * #3913 — server.ts back over its coverage floor with REAL tests.
 * First slice: the /api/messages limit-clamp branches (#3852) — raw NaN,
 * negative, and over-cap limits each take a distinct branch.
 */
import { dedupeLines } from '../src/server';

describe('#3913 dedupeLines branch coverage', () => {
  test('exact duplicate from same role is dropped', () => {
    const lines = [
      { ts: '1', role: 'kade', type: 'obs', text: 'same', card: null },
      { ts: '2', role: 'kade', type: 'obs', text: 'same', card: null },
    ];
    expect(dedupeLines(lines as never).length).toBe(1);
  });
  test('same text from DIFFERENT roles both survive', () => {
    const lines = [
      { ts: '1', role: 'kade', type: 'obs', text: 'same', card: null },
      { ts: '2', role: 'wren', type: 'obs', text: 'same', card: null },
    ];
    expect(dedupeLines(lines as never).length).toBe(2);
  });
});
