import Database from 'better-sqlite3';
import { buildSearchMeta, addStaleHeader, SOURCE_CADENCE, STALE_THRESHOLD_MS } from '../src/search-meta';
import * as ela from '../src/eventloop-alert';

function freshDb() {
  const db = new Database(':memory:');
  db.exec('CREATE TABLE watermarks (source TEXT PRIMARY KEY, last_indexed TEXT)');
  return db;
}

function fakeRes(): { headers: Record<string, string>; setHeader: (k: string, v: string) => void } {
  const headers: Record<string, string> = {};
  return {
    headers,
    setHeader: (k: string, v: string) => { headers[k] = v; },
  };
}

describe('SOURCE_CADENCE', () => {
  it('maps realtime-ish sources to 1h (3600 s)', () => {
    expect(SOURCE_CADENCE.claude).toBe(3600);
    expect(SOURCE_CADENCE.spine).toBe(3600);
  });

  it('maps brief/decision/clearing to 1d', () => {
    expect(SOURCE_CADENCE.brief).toBe(86400);
    expect(SOURCE_CADENCE.decision).toBe(86400);
    expect(SOURCE_CADENCE.clearing).toBe(86400);
  });
});

describe('addStaleHeader', () => {
  it('does nothing when there are no watermarks', () => {
    const db = freshDb();
    const res = fakeRes();
    addStaleHeader(res as any, db as any);
    expect(res.headers['X-Chorus-Stale']).toBeUndefined();
  });

  it('sets X-Chorus-Stale when the latest watermark exceeds STALE_THRESHOLD_MS', () => {
    const db = freshDb();
    const oldIso = new Date(Date.now() - 2 * STALE_THRESHOLD_MS).toISOString();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('claude', oldIso);
    const res = fakeRes();
    addStaleHeader(res as any, db as any);
    expect(res.headers['X-Chorus-Stale']).toBe('true');
  });

  it('does not set the header when the latest watermark is within threshold', () => {
    const db = freshDb();
    const halfThresholdAgo = new Date(Date.now() - STALE_THRESHOLD_MS / 2).toISOString();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('claude', halfThresholdAgo);
    const res = fakeRes();
    addStaleHeader(res as any, db as any);
    expect(res.headers['X-Chorus-Stale']).toBeUndefined();
  });
});

describe('buildSearchMeta', () => {
  it('returns the expected shape with no db and no results', () => {
    const meta = buildSearchMeta([]);
    expect(meta).toEqual({
      domain_coverage: 1,
      newest_result_age_s: 0,
      stale: false,
      sources: {},
      schema_version: '1.0.0',
    });
  });

  it('counts result.source occurrences into sources', () => {
    const meta = buildSearchMeta([
      { source: 'claude' }, { source: 'claude' }, { source: 'spine' },
    ]);
    expect(meta.sources).toEqual({ claude: 2, spine: 1 });
  });

  it('falls back to result.domain when source is missing', () => {
    const meta = buildSearchMeta([{ domain: 'chorus' }, {}]);
    expect(meta.sources).toEqual({ chorus: 1, unknown: 1 });
  });

  it('computes newest_result_age_s from the most-recent timestamp', () => {
    const nowIso = new Date().toISOString();
    const olderIso = new Date(Date.now() - 60_000).toISOString();
    const meta = buildSearchMeta([
      { source: 'x', timestamp: olderIso },
      { source: 'x', timestamp: nowIso },
    ]);
    expect(meta.newest_result_age_s).toBeLessThan(5);
  });

  it('marks stale when the newest result is older than 24h', () => {
    const oldIso = new Date(Date.now() - 25 * 3600_000).toISOString();
    const meta = buildSearchMeta([{ source: 'x', timestamp: oldIso }]);
    expect(meta.stale).toBe(true);
  });

  it('computes domain_coverage from watermarks when db is provided', () => {
    const db = freshDb();
    const fresh = new Date().toISOString();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('claude:session-1', fresh);
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('spine:event-1', fresh);
    const meta = buildSearchMeta([], db as any);
    expect(meta.domain_coverage).toBe(1);
  });

  it('drops domain_coverage when a source is beyond 2x its cadence', () => {
    const db = freshDb();
    const fresh = new Date().toISOString();
    const staleClaude = new Date(Date.now() - 3 * SOURCE_CADENCE.claude * 1000 - 10_000).toISOString();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('claude:a', staleClaude);
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('spine:b', fresh);
    const meta = buildSearchMeta([], db as any);
    expect(meta.domain_coverage).toBeLessThan(1);
  });

  it('aggregates artifact:<kind> sources under a composite key', () => {
    const db = freshDb();
    const fresh = new Date().toISOString();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('artifact:decision:one', fresh);
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('artifact:decision:two', fresh);
    const meta = buildSearchMeta([], db as any);
    expect(meta.domain_coverage).toBe(1);
  });

  it('returns domain_coverage=1 when the db query throws', () => {
    const brokenDb = {
      prepare: () => { throw new Error('boom'); },
    };
    const meta = buildSearchMeta([], brokenDb as any);
    expect(meta.domain_coverage).toBe(1);
  });
});

// #3400 AC1 (PROVE-FIRST) — the watermarks full-table scan runs synchronously on
// the search-serve path during response formatting, untagged, which is exactly why
// eventloop.blocked alerts log op="unknown". Tagging it makes the NEXT block alert
// name op="search-meta.watermarks" (CONFIRMED root) or stay "unknown" (REFUTED).
// This test asserts the instrumentation is wired; the live verdict is the Loki meter.
describe('#3400 AC1 — watermarks op instrumentation', () => {
  it('tags the watermarks full-table scan as search-meta.watermarks, then resets the op', () => {
    const spy = jest.spyOn(ela, 'setCurrentOp');
    const db = freshDb();
    db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)')
      .run('claude', new Date().toISOString());
    try {
      buildSearchMeta([], db as any);
      expect(spy).toHaveBeenCalledWith('search-meta.watermarks');
      // op is reset so it never leaks past the query (no stale attribution)
      expect(spy).toHaveBeenLastCalledWith(null);
    } finally {
      spy.mockRestore();
      db.close();
    }
  });
});
