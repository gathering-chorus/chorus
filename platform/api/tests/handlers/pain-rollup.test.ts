/**
 * pain rollup v1.1 (#3029) — unit tests for the raw-line rollup ported from the
 * proven :8899 sketch (logform.py). Locks the contract the v1 server-side count
 * threw away: per-class cards / latest / sample-detail, freeform window, the
 * line-anchored failure filter (excludes the pulse-heartbeat FPs), synthetic
 * test-card exclusion (99998 never ranks #1), and per-product split.
 */
import {
  rollupRawLines,
  windowToSeconds,
  lineMatchesPainFilter,
  PAIN_EVENT_SUFFIXES,
} from '../../src/handlers/logs-query';

// Build a Loki query_range-shaped body from [tsNs, jsonObj] rows.
function lokiBody(rows: Array<[string, Record<string, unknown>]>) {
  return { data: { result: [{ values: rows.map(([ts, o]) => [ts, JSON.stringify(o)] as [string, string]) }] } };
}

describe('windowToSeconds — freeform window (matches the :8899 sketch)', () => {
  it('parses <n>[smhd]', () => {
    expect(windowToSeconds('30m')).toBe(1800);
    expect(windowToSeconds('12h')).toBe(43200);
    expect(windowToSeconds('1d')).toBe(86400);
    expect(windowToSeconds('7d')).toBe(604800);
  });
  it('rejects junk', () => {
    expect(windowToSeconds('bogus')).toBeNull();
    expect(windowToSeconds('12')).toBeNull();
    expect(windowToSeconds('12y')).toBeNull();
  });
});

describe('lineMatchesPainFilter — line-anchored, excludes the false-positives', () => {
  it('matches real failure events (event value ends in .failed/.refused/.error)', () => {
    expect(lineMatchesPainFilter('{"event":"demo.show.failed","role":"kade"}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"mcp.tool.error"}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"chorus_acp.refused"}')).toBe(true);
  });
  it('excludes the high-volume non-failures the substring filter false-counted', () => {
    expect(lineMatchesPainFilter('{"event":"heartbeat.probe"}')).toBe(false);
    expect(lineMatchesPainFilter('{"event":"system.heartbeat"}')).toBe(false);
    expect(lineMatchesPainFilter('{"event":"clearing.probe.passed"}')).toBe(false);
  });
  it('PAIN_EVENT_SUFFIXES is the closed set', () => {
    expect([...PAIN_EVENT_SUFFIXES]).toEqual(['.failed', '.refused', '.error']);
  });
});

describe('rollupRawLines — group with cards/latest/detail, drop synthetic cards', () => {
  const rows: Array<[string, Record<string, unknown>]> = [
    ['3000000000000000000', { role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started', card_id: 99998 }], // synthetic — excluded
    ['3000000000000000001', { role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started', card_id: 99998 }], // synthetic — excluded
    ['3000000000000000002', { role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started', card_id: 3023 }],
    ['3000000000000000003', { role: 'silas', event: 'session.context.error' }],
    ['3000000000000000004', { role: 'silas', event: 'session.context.error' }],
    ['3000000000000000005', { role: 'system', event: 'crawler.domain.failed', detail: 'timeout connecting to host\nsecond line should be dropped' }],
  ];

  it('groups by role·event·reason, ranks by count, and 99998 never ranks #1', () => {
    const { classes, total } = rollupRawLines(lokiBody(rows));
    expect(total).toBe(4); // the 2 synthetic-card lines are dropped
    expect(classes[0]).toMatchObject({ role: 'silas', event: 'session.context.error', count: 2 });
    const demo = classes.find((c) => c.event === 'demo.show.failed')!;
    expect(demo.count).toBe(1);           // only the real card 3023, not the 2 from 99998
    expect(demo.cards).toEqual(['3023']); // the synthetic card never appears in Cards
  });

  it('captures latest, first-line sample detail, and per-product split', () => {
    const { classes, byProduct } = rollupRawLines(lokiBody(rows));
    const crawler = classes.find((c) => c.event === 'crawler.domain.failed')!;
    expect(crawler.detail).toContain('timeout connecting to host');
    expect(crawler.detail).not.toContain('second line'); // only the first line of detail
    expect(crawler.latest).toMatch(/\d\d-\d\d \d\d:\d\d:\d\d/); // MM-DD HH:MM:SS
    expect(crawler.product).toBe('Gathering'); // crawler.* → Gathering
    expect(byProduct.Chorus).toBe(3);          // demo(1) + session(2)
    expect(byProduct.Gathering).toBe(1);       // crawler(1)
  });

  it('role filter narrows to one role', () => {
    const { classes, total } = rollupRawLines(lokiBody(rows), { role: 'silas' });
    expect(total).toBe(2);
    expect(classes.every((c) => c.role === 'silas')).toBe(true);
  });

  it('is empty-safe on malformed / empty bodies', () => {
    expect(rollupRawLines({}).classes).toEqual([]);
    expect(rollupRawLines({ data: { result: [] } }).total).toBe(0);
    expect(rollupRawLines(null).total).toBe(0);
  });
});
