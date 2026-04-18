/**
 * #2168 AC-14 — hybrid search RRF weighting.
 *
 * Unit tests for mergeRRF + hasExactToken. Default semantic-first, FTS-boost
 * when the query carries exact tokens (card IDs, file paths, symbol names).
 */

import { mergeRRF, hasExactToken } from '../src/search-rrf';

describe('hasExactToken', () => {
  test('card ID #2168 is exact token', () => {
    expect(hasExactToken('what happened on #2168')).toBe(true);
  });
  test('dotted filename is exact token', () => {
    expect(hasExactToken('look at server.ts line 500')).toBe(true);
    expect(hasExactToken('pulse.rs behavior')).toBe(true);
  });
  test('path segment is exact token', () => {
    expect(hasExactToken('see platform/api/src')).toBe(true);
  });
  test('underscored symbol is exact token', () => {
    expect(hasExactToken('check query_chorus_hybrid')).toBe(true);
  });
  test('plain natural language is not exact', () => {
    expect(hasExactToken('what is pulse doing wrong')).toBe(false);
    expect(hasExactToken('how do we tune the envelope')).toBe(false);
  });
});

describe('mergeRRF — semantic-first by default', () => {
  const fts = [
    { id: 1, content: 'fts-a' },
    { id: 2, content: 'fts-b' },
  ];
  const sem = [
    { msg_id: 10, source: 'x', channel: '', role: '', content: 'sem-a', timestamp: '', score: 0.9 },
    { msg_id: 11, source: 'x', channel: '', role: '', content: 'sem-b', timestamp: '', score: 0.8 },
  ];

  test('natural-language query ranks semantic results first', () => {
    const out = mergeRRF(fts, sem, 4, 'how does pulse work');
    expect(out[0]).toMatchObject({ content: 'sem-a' });
    expect(out[1]).toMatchObject({ content: 'sem-b' });
  });

  test('exact-token query ranks FTS results first', () => {
    const out = mergeRRF(fts, sem, 4, 'see card #2168 and server.ts');
    expect(out[0]).toMatchObject({ content: 'fts-a' });
    expect(out[1]).toMatchObject({ content: 'fts-b' });
  });

  test('appearing in both still merges (score adds)', () => {
    const shared = [{ id: 10, content: 'shared' }];
    const out = mergeRRF(shared, sem, 3, 'how does pulse work');
    expect(out[0].content).toBe('shared');
    expect(out.length).toBe(2);
  });

  test('empty query falls back to semantic-first', () => {
    const out = mergeRRF(fts, sem, 4, '');
    expect(out[0]).toMatchObject({ content: 'sem-a' });
  });
});
