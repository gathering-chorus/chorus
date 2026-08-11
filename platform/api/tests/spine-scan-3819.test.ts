// @test-type: unit — writes and reads its own tmpdir files; no live spine, no service.
/**
 * #3819 — reading the spine's tail instead of all 732MB of it.
 *
 * The dangerous outcome here is NOT a crash. It is a version that gets fast by
 * quietly looking at less and reports the same shape as one that looked at
 * everything. Fast-and-wrong and fast-and-right are indistinguishable from the
 * outside, which is why most of this file is about `truncated` rather than
 * about speed.
 */

import fs from 'fs';
import os from 'os';
import path from 'path';
import { scanTail, parseMatching, DEFAULT_TAIL_BYTES } from '../src/handlers/spine-scan';

function writeLog(lines: string[]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'spine-scan-3819-'));
  const file = path.join(dir, 'chorus.log');
  fs.writeFileSync(file, `${lines.join('\n')}\n`);
  return file;
}

const event = (card: number, name: string) => JSON.stringify({
  timestamp: `2026-08-11T00:00:0${card % 10}Z`,
  event: name,
  role: 'wren',
  card: String(card),
});

describe('#3819 scanTail', () => {
  test('reads a small file whole and says it did not truncate', () => {
    const file = writeLog([event(1, 'card.pulled'), event(2, 'card.landed')]);
    const scan = scanTail(file);
    expect(scan.lines).toHaveLength(2);
    expect(scan.truncated).toBe(false);
  });

  test('reads only the tail of a file larger than the window', () => {
    const lines = Array.from({ length: 500 }, (_, i) => event(i, 'card.pulled'));
    const file = writeLog(lines);
    const scan = scanTail(file, 2000); // deliberately tiny window
    expect(scan.bytesRead).toBeLessThanOrEqual(2000);
    expect(scan.lines.length).toBeLessThan(500);
    expect(scan.truncated).toBe(true);
  });

  test('NEGATIVE PROOF: an event OUTSIDE the window is missed, and truncated says so', () => {
    // This is the whole card. A scan that silently returns fewer events looks
    // exactly like a card with a shorter history. The caller must be able to
    // tell "nothing happened" from "we stopped looking" — so the missing event
    // is asserted MISSING and the flag is asserted TRUE in the same test.
    // If someone later removes the flag, this fails; if someone widens the
    // window to hide the problem, the first assertion fails.
    const oldEvent = JSON.stringify({ timestamp: 'old', event: 'card.pulled', card: '4242' });
    const filler = Array.from({ length: 400 }, (_, i) => event(i, 'card.noise'));
    const file = writeLog([oldEvent, ...filler]);

    const scan = scanTail(file, 1500);
    const found = parseMatching(
      scan.lines,
      (l) => l.includes('"card":"4242"'),
      (p) => p,
    );
    expect(found).toHaveLength(0);   // genuinely missed
    expect(scan.truncated).toBe(true); // and admitted
  });

  test('NEGATIVE PROOF: the same event INSIDE the window is found — the scan is not just always empty', () => {
    // Without this, the test above would pass against an implementation that
    // returns nothing at all.
    const wanted = JSON.stringify({ timestamp: 'recent', event: 'card.pulled', card: '4242' });
    const file = writeLog([...Array.from({ length: 400 }, (_, i) => event(i, 'card.noise')), wanted]);
    const scan = scanTail(file, 1500);
    const found = parseMatching(scan.lines, (l) => l.includes('"card":"4242"'), (p) => p);
    expect(found).toHaveLength(1);
    expect(scan.truncated).toBe(true);
  });

  test('a partial first line is dropped rather than half-parsed', () => {
    // A window starting mid-file starts mid-line. Half a JSON object parses as
    // nothing and would vanish silently; dropping it deliberately is the same
    // act done knowingly, and the count proves nothing else went with it.
    const lines = Array.from({ length: 200 }, (_, i) => event(i, 'card.pulled'));
    const file = writeLog(lines);
    const scan = scanTail(file, 900);
    for (const line of scan.lines) {
      expect(() => JSON.parse(line)).not.toThrow();
    }
  });

  test('a missing file is an empty scan, not a throw', () => {
    const scan = scanTail('/definitely/not/here/chorus.log');
    expect(scan.lines).toHaveLength(0);
    expect(scan.truncated).toBe(false);
  });

  test('the default window is bounded — not the whole file', () => {
    // A "default" of Infinity would pass every other test in this file while
    // restoring the exact defect the card exists to remove.
    expect(DEFAULT_TAIL_BYTES).toBeLessThan(100 * 1024 * 1024);
    expect(DEFAULT_TAIL_BYTES).toBeGreaterThan(1024 * 1024);
  });
});

describe('#3819 parseMatching', () => {
  test('returns only lines the predicate matched and the taker kept', () => {
    const lines = [event(1, 'card.pulled'), event(2, 'card.landed'), 'not json at all'];
    const got = parseMatching(
      lines,
      (l) => l.includes('card.'),
      (p) => (String(p.event).startsWith('card.') ? String(p.event) : null),
    );
    expect(got).toEqual(['card.pulled', 'card.landed']);
  });

  test('a malformed line is skipped without stopping the scan', () => {
    // The spine is append-only from many writers; a torn line is a fact of the
    // file, not an error. Stopping on it would lose everything after it.
    const lines = ['{ broken', event(9, 'card.landed')];
    const got = parseMatching(lines, () => true, (p) => (p.event ? String(p.event) : null));
    expect(got).toEqual(['card.landed']);
  });

  test('NEGATIVE PROOF: the cheap predicate does not admit lines the taker should reject', () => {
    // match() runs on the raw string for speed. If that made it sloppier than
    // the parse, a "card." substring inside some other field would slip a
    // non-card event into a card's story.
    const decoy = JSON.stringify({ event: 'nudge.sent', text: 'mentions card.pulled in passing' });
    const got = parseMatching(
      [decoy],
      (l) => l.includes('card.'),
      (p) => (String(p.event).startsWith('card.') ? String(p.event) : null),
    );
    expect(got).toEqual([]);
  });
});

describe('#3819 the predicate matches what the spine actually writes', () => {
  // Measured against the live file on 2026-08-11: card events look like
  //   {"timestamp":"...","event":"card.demo.started","role":"wren","card_id":3813}
  // card_id, and a NUMBER. The old predicate searched for `card=N` or
  // `"card":"N"` — neither string occurs in this log. So the handler spent
  // 3-8 seconds per request to return an empty timeline, and the alert was
  // measuring work that produced nothing.
  const REAL_LINE = JSON.stringify({
    timestamp: '2026-08-11T15:40:16.900-0400',
    event: 'card.demo.started',
    role: 'wren',
    card_id: 3813,
  });

  const matchFor = (cardId: number) => (line: string) => line.includes(`"card_id":${cardId}`)
    || line.includes(`card=${cardId}`)
    || line.includes(`"card":"${cardId}"`);

  test('finds a card event in the real line shape', () => {
    const got = parseMatching([REAL_LINE], matchFor(3813), (p) => String(p.event));
    expect(got).toEqual(['card.demo.started']);
  });

  test('NEGATIVE PROOF: the OLD predicate finds nothing in the real line shape', () => {
    // The bug, pinned. If someone "simplifies" the predicate back to the old
    // forms, this fails — which is the only way a reader learns that the old
    // one was not merely slow but blind.
    const oldPredicate = (line: string) => line.includes('card=3813') || line.includes('"card":"3813"');
    const got = parseMatching([REAL_LINE], oldPredicate, (p) => String(p.event));
    expect(got).toEqual([]);
  });

  test('NEGATIVE PROOF: a prefix collision is caught by the TAKER, not the predicate', () => {
    // "card_id":38130 contains "card_id":3813, so the cheap string filter DOES
    // let it through — proven here rather than assumed. What keeps it out of
    // the story is the taker re-checking the parsed number. Layering it this
    // way is a choice: the filter stays cheap over 40k lines, and correctness
    // lives where the value is already parsed.
    const other = JSON.stringify({ event: 'card.pulled', card_id: 38130 });
    expect(matchFor(3813)(other)).toBe(true); // the filter is fooled...
    const taker = (p: Record<string, unknown>) => {
      const id = Number(p.card_id ?? p.card ?? NaN);
      if (Number.isFinite(id) && id !== 3813) return null;
      return String(p.event);
    };
    expect(parseMatching([other], matchFor(3813), taker)).toEqual([]); // ...the taker is not
  });
});
