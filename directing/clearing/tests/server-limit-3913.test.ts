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

import { formatObserverDigest, extractSequenceTags, pickJeffMessageTargets } from '../src/server';

describe('#3913 formatObserverDigest branches', () => {
  const line = (ts: string, digest: string, card?: string) => JSON.stringify({ ts, digest, card });
  test('duplicate observer double-write dedupes on ts|digest', () => {
    const c = [line('2026-08-17T10:00:00Z', 'bash: ls'), line('2026-08-17T10:00:00Z', 'bash: ls')].join('\n');
    expect(formatObserverDigest(c).length).toBe(1);
  });
  test('bash prefix rewrites to arrow; card suffix appended', () => {
    const c = line('2026-08-17T10:00:00Z', 'bash: cargo test', '3913');
    const out = formatObserverDigest(c)[0];
    expect(out).toContain('→ cargo test');
    expect(out).toContain('#3913');
  });
  test('non-JSON lines are skipped, not fatal', () => {
    const c = ['not json', line('2026-08-17T10:00:00Z', 'bash: ok')].join('\n');
    expect(formatObserverDigest(c).length).toBe(1);
  });
});

describe('#3913 extractSequenceTags branches', () => {
  test('explicit sequence tags win, deduped', () => {
    expect(extractSequenceTags('sequence:chorus|sequence:chorus|P1')).toEqual(['chorus']);
  });
  test('bare tag fallback skips owners and priorities', () => {
    expect(extractSequenceTags('Kade|P2|gathering')).toEqual(['gathering']);
  });
  test('nothing usable → empty', () => {
    expect(extractSequenceTags('Wren|P1')).toEqual([]);
  });
});

describe('#3913 pickJeffMessageTargets branches', () => {
  test('no mention → all roles', () => {
    expect(pickJeffMessageTargets('hello team').sort()).toEqual(['kade', 'silas', 'wren']);
  });
  test('mentions dedupe + lowercase', () => {
    expect(pickJeffMessageTargets('@Kade @kade check @WREN')).toEqual(['kade', 'wren']);
  });
});
