// #3051 — /search must never drop to a `content LIKE '%q%'` full scan over 1.26M
// rows (synchronous, ~3.6s, freezes the spine). The cause was FTS5 MATCH throwing
// on special chars, falling back to LIKE. toFtsMatchQuery quote-wraps each token
// so MATCH is always valid syntax — no throw, no fallback. Red before the fn exists.

import { toFtsMatchQuery } from '../../src/handlers/chorus-search';

describe('toFtsMatchQuery (#3051 — kill the LIKE-fallback full scan)', () => {
  it('quotes each token so plain queries are valid FTS5', () => {
    expect(toFtsMatchQuery('wren pipeline deploy')).toBe('"wren" "pipeline" "deploy"');
  });

  it('strips the FTS5-special chars that used to throw MATCH (the messy query)', () => {
    expect(toFtsMatchQuery('context: (search) "loki" — errors')).toBe(
      '"context" "search" "loki" "errors"',
    );
  });

  it('produces only quoted literals for any operator/punctuation input (never throws)', () => {
    for (const q of ['AND OR NOT', 'a* b^ c~', '() "" :', "jeff's work", 'foo-bar baz']) {
      const out = toFtsMatchQuery(q).trim();
      expect(out === '' || /^("[^"]+"\s?)+$/.test(out)).toBe(true);
    }
  });

  it('returns empty for all-punctuation input (caller returns no rows, no scan)', () => {
    expect(toFtsMatchQuery('()[]:*^ "" --')).toBe('');
  });
});
