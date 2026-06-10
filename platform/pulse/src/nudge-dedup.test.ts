// #3335 Pattern 7 — dedup window tests (jest/ts-jest, the pulse runner).
import { dedupeKey, seenRecently } from './nudge-dedup';

describe('nudge dedup (Pattern 7)', () => {
  it('first send is not a duplicate; an identical send within the window IS', () => {
    const seen = new Map<string, number>();
    const k = dedupeKey('wren', 'silas', 'ack required');
    expect(seenRecently(k, 1000, seen, 10_000)).toBe(false); // first
    expect(seenRecently(k, 1500, seen, 10_000)).toBe(true);  // 0.5s later → dup
  });

  it('an identical send AFTER the window is allowed (legitimate re-send)', () => {
    const seen = new Map<string, number>();
    const k = dedupeKey('wren', 'silas', 'ping');
    expect(seenRecently(k, 1000, seen, 10_000)).toBe(false);
    expect(seenRecently(k, 12_000, seen, 10_000)).toBe(false); // 11s later → allowed
  });

  it('different (from,to,content) are independent — no cross-suppression', () => {
    const seen = new Map<string, number>();
    expect(seenRecently(dedupeKey('wren', 'silas', 'a'), 1000, seen, 10_000)).toBe(false);
    expect(seenRecently(dedupeKey('wren', 'kade', 'a'), 1000, seen, 10_000)).toBe(false); // diff recipient
    expect(seenRecently(dedupeKey('wren', 'silas', 'b'), 1000, seen, 10_000)).toBe(false); // diff content
  });

  it('prunes stale entries so the map cannot grow unbounded', () => {
    const seen = new Map<string, number>();
    seenRecently(dedupeKey('a', 'b', 'old'), 1000, seen, 10_000);
    expect(seen.size).toBe(1);
    // a later call past the window prunes the stale one before recording the new
    seenRecently(dedupeKey('c', 'd', 'new'), 20_000, seen, 10_000);
    expect(seen.has(dedupeKey('a', 'b', 'old'))).toBe(false);
    expect(seen.has(dedupeKey('c', 'd', 'new'))).toBe(true);
  });
});
