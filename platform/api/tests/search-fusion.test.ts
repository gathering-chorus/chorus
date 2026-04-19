import { mergeUnified, resolveSearchLimit, enrichHit } from '../src/search-fusion';

describe('mergeUnified', () => {
  it('returns empty array when all inputs are empty', () => {
    expect(mergeUnified([], [], [], 10)).toEqual([]);
  });

  it('carries FTS result into output with the chorus: keyed id', () => {
    const fts = [{ id: 42, source: 'slack', role: 'jeff', content: 'hi', timestamp: '2026-04-18' }];
    const merged = mergeUnified(fts, [], [], 10);
    expect(merged).toHaveLength(1);
    expect(merged[0].id).toBe(42);
    expect(merged[0]._sources).toEqual(['fts']);
    expect(merged[0]._rrf_score).toBeGreaterThan(0);
  });

  it('combines FTS and semantic results for the same msg_id under one entry', () => {
    const fts = [{ id: 7, source: 'slack', role: 'kade', content: 'hello', timestamp: 't' }];
    const sem = [{ msg_id: 7, source: 'slack', channel: 'c', role: 'kade', content: 'hello', timestamp: 't', score: 0.9 }];
    const merged = mergeUnified(fts, sem, [], 10);
    expect(merged).toHaveLength(1);
    expect(merged[0]._sources.sort()).toEqual(['fts', 'semantic']);
  });

  it('keeps sparql results distinct from chorus results via uri vs id key', () => {
    const fts = [{ id: 1, content: 'a', timestamp: 't' }];
    const sparql = [{ uri: 'urn:x', type: 'Thing', domain: 'chorus', label: 'x', content: 'x', score: 0.5 }];
    const merged = mergeUnified(fts, [], sparql, 10);
    expect(merged).toHaveLength(2);
    const chorusEntry = merged.find(e => e.source === 'chorus');
    const sparqlEntry = merged.find(e => e.source === 'sparql');
    expect(chorusEntry?.id).toBe(1);
    expect(sparqlEntry?.uri).toBe('urn:x');
  });

  it('applies RRF scoring: earlier rank contributes more than later', () => {
    const fts = [
      { id: 1, content: 'first' },
      { id: 2, content: 'second' },
    ];
    const merged = mergeUnified(fts, [], [], 10);
    const first = merged.find(r => r.id === 1)!;
    const second = merged.find(r => r.id === 2)!;
    expect(first._rrf_score).toBeGreaterThan(second._rrf_score);
  });

  it('sorts results by RRF score descending', () => {
    // Same id in FTS and semantic gets double-score and rises above solo hits.
    const fts = [{ id: 99, content: 'both' }, { id: 1, content: 'solo' }];
    const sem = [{ msg_id: 99, source: 's', channel: 'c', role: 'r', content: 'both', timestamp: 't', score: 1 }];
    const merged = mergeUnified(fts, sem, [], 10);
    expect(merged[0].id).toBe(99);
  });

  it('respects the limit argument', () => {
    const fts = Array.from({ length: 20 }, (_, i) => ({ id: i, content: String(i) }));
    const merged = mergeUnified(fts, [], [], 5);
    expect(merged).toHaveLength(5);
  });

  it('uses the custom k parameter in RRF', () => {
    const fts = [{ id: 1, content: 'x' }];
    const low_k = mergeUnified(fts, [], [], 10, 10);
    const high_k = mergeUnified(fts, [], [], 10, 1000);
    // Lower k produces a higher 1/(k+i+1) score.
    expect(low_k[0]._rrf_score).toBeGreaterThan(high_k[0]._rrf_score);
  });
});

describe('resolveSearchLimit', () => {
  it('returns default when limit is undefined', () => {
    const r = resolveSearchLimit(undefined);
    expect(r).toEqual({ limit: 5, explicit: false });
  });

  it('returns default when limit is not a number', () => {
    expect(resolveSearchLimit('banana')).toEqual({ limit: 5, explicit: false });
  });

  it('returns default when limit is less than 1', () => {
    expect(resolveSearchLimit('0')).toEqual({ limit: 5, explicit: false });
    expect(resolveSearchLimit('-4')).toEqual({ limit: 5, explicit: false });
  });

  it('returns the parsed value when valid', () => {
    expect(resolveSearchLimit('12')).toEqual({ limit: 12, explicit: true });
  });

  it('caps at SEARCH_MAX_LIMIT (100)', () => {
    expect(resolveSearchLimit('500')).toEqual({ limit: 100, explicit: true });
  });

  it('marks explicit true even when capped', () => {
    const r = resolveSearchLimit('250');
    expect(r.explicit).toBe(true);
  });
});

describe('enrichHit', () => {
  it('adds freshness_s = 0 when timestamp missing', () => {
    const r = enrichHit({ content: 'x' }, 1_000_000);
    expect(r.freshness_s).toBe(0);
  });

  it('computes freshness_s as seconds between now and timestamp', () => {
    const ts = '2026-04-18T00:00:00Z';
    const now = new Date('2026-04-18T00:01:00Z').getTime();
    const r = enrichHit({ timestamp: ts, content: 'x' }, now);
    expect(r.freshness_s).toBe(60);
  });

  it('clamps negative values to 0 for future timestamps', () => {
    const future = '2099-01-01T00:00:00Z';
    const r = enrichHit({ timestamp: future }, Date.now());
    expect(r.freshness_s).toBe(0);
  });

  it('returns freshness_s = 0 for unparseable timestamp', () => {
    const r = enrichHit({ timestamp: 'not-a-date' }, 1_000_000);
    expect(r.freshness_s).toBe(0);
  });

  it('preserves all original hit fields', () => {
    const hit = { id: 1, content: 'x', role: 'kade', timestamp: '2026-04-18T00:00:00Z' };
    const r = enrichHit(hit, new Date('2026-04-18T00:00:30Z').getTime());
    expect(r.id).toBe(1);
    expect(r.content).toBe('x');
    expect(r.role).toBe('kade');
    expect(r.freshness_s).toBe(30);
  });
});
