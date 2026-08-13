// @test-type: unit — signal is fixture-data: pure counter, no io, no room
//
// #3851 — the Clearing/Buzz outbound path. Jeff, 2026-08-13: the cap holds on
// ALL messages. The room was a send path nobody had capped.
import { checkCap, countWords } from '../src/word-cap';

const CAP = 100;
const words = (n: number) => Array.from({ length: n }, (_, i) => `w${i}`).join(' ');

describe('#3851 room outbound cap', () => {
  it('an at-cap role message passes', () => {
    expect(checkCap(words(CAP), CAP).ok).toBe(true);
  });

  // NEGATIVE PROOF (#3734) — the gate must refuse on this path, not just exist.
  it('REFUSES one word over, and names the count', () => {
    const v = checkCap(words(CAP + 1), CAP);
    expect(v.ok).toBe(false);
    if (v.ok) throw new Error('unreachable');
    expect(v.words).toBe(CAP + 1);
    expect(v.message).toMatch(/nothing was truncated/);
  });

  it('a long fenced receipt does not push a short message over', () => {
    const fence = '```\n' + 'x '.repeat(400) + '\n```';
    expect(checkCap(`short note\n${fence}`, CAP).ok).toBe(true);
  });

  // The counter must be the SHARED one — a second definition of "a word" would
  // bounce Jeff on one surface and not another.
  it('uses the same counting rule as every other surface', () => {
    expect(countWords('--- Wren | 2026-08-13 | #3851 ---\n\nthree real words')).toBe(3);
    expect(countWords('see https://example.com/x now')).toBe(2);
  });
});
