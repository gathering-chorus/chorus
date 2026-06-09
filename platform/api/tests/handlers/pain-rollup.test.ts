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
  it('every PAIN_EVENT_SUFFIXES suffix actually matches via the filter (#3165 adds .rolledback, #3281 adds .blocked)', () => {
    for (const sfx of PAIN_EVENT_SUFFIXES) {
      expect(lineMatchesPainFilter(`{"event":"some.op${sfx}"}`)).toBe(true);
    }
    expect([...PAIN_EVENT_SUFFIXES]).toEqual(['.failed', '.refused', '.error', '.rolledback', '.blocked']);
  });
  // #3281 — agent-side blocks become countable. card.quality.blocked (AC/quality
  // gate denials on card add — the "AC-checkbox block" from the card Experience)
  // is the only .blocked event today (85 in a recent log window), carries role +
  // gate=<reason>, but was invisible to the pain board. Now counted via .blocked.
  it('counts card.quality.blocked — the AC-format / quality denial (#3281)', () => {
    expect(lineMatchesPainFilter('{"event":"card.quality.blocked","role":"silas","gate":"experience_missing"}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"card.quality.blocked","role":"wren","gate":"now_description_incomplete"}')).toBe(true);
  });
});

// #3165 — the rollup keys on DISPOSITION (deny/refuse/rollback) + level, not just
// event-name suffix. Upstream blocker for the team's emit slice (Kade #3161-3164,
// #2834 gate-denies): those emits carry disposition=deny/refuse/rollback but no
// matching event-suffix, so a suffix-only filter counted none of them.
describe('lineMatchesPainFilter — disposition/level keying (#3165)', () => {
  it('counts existing deploy.rolledback via the .rolledback suffix (it carries reason, no disposition field)', () => {
    expect(lineMatchesPainFilter('{"event":"deploy.rolledback","reason":"cdhash-divergence","card_id":3165}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"deploy.rolledback","reason":"consumer-resolves-stale-lib"}')).toBe(true);
  });
  it('counts disposition-tagged failures regardless of event name (the emit slice)', () => {
    expect(lineMatchesPainFilter('{"event":"guard.canonical_write.blocked","disposition":"deny"}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"werk_pull.bailed","disposition":"refuse"}')).toBe(true);
    expect(lineMatchesPainFilter('{"event":"some.op","disposition":"rollback"}')).toBe(true);
  });
  it('does NOT count bare level=error in v1 — deferred (live Loki: 166/193 are gathering-app/daemon logs with no event/role/card_id; they need their own grouping, not the spine schema)', () => {
    expect(lineMatchesPainFilter('{"event":"some.op","level":"error","card_id":1}')).toBe(false);
    expect(lineMatchesPainFilter('{"level":"error","msg":"boom"}')).toBe(false);
    // but a level=error line that ALSO has a pain-suffix event still counts (via the suffix)
    expect(lineMatchesPainFilter('{"event":"mcp.cards.add.failed","level":"error"}')).toBe(true);
  });
  it('does NOT reopen the #3029 over-count (heartbeat, grafana, level=warn/info, benign disposition)', () => {
    expect(lineMatchesPainFilter('{"event":"heartbeat.probe"}')).toBe(false);
    expect(lineMatchesPainFilter('{"event":"system.heartbeat","level":"info"}')).toBe(false);
    expect(lineMatchesPainFilter('{"event":"clearing.probe.passed","level":"warn"}')).toBe(false);
    expect(lineMatchesPainFilter('{"event":"deploy.completed","disposition":"ok"}')).toBe(false);
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

  it('#3165 — a disposition line with no card_id is counted + surfaced (unattributed), not dropped or mis-grouped', () => {
    const noCard: Array<[string, Record<string, unknown>]> = [
      ['3000000000000000010', { role: 'silas', event: 'guard.canonical_write.blocked', disposition: 'deny' }],  // no card_id
      ['3000000000000000011', { role: 'silas', event: 'werk_pull.bailed', disposition: 'refuse' }],            // no card_id
    ];
    const { classes, total } = rollupRawLines(lokiBody(noCard));
    expect(total).toBe(2); // counted, not silently dropped
    const deny = classes.find((c) => c.event === 'guard.canonical_write.blocked')!;
    expect(deny.count).toBe(1);
    expect(deny.cards).toEqual([]); // unattributed — surfaced with no card, never mis-attributed
  });

  it('is empty-safe on malformed / empty bodies', () => {
    expect(rollupRawLines({}).classes).toEqual([]);
    expect(rollupRawLines({ data: { result: [] } }).total).toBe(0);
    expect(rollupRawLines(null).total).toBe(0);
  });

  // #3149-fix — read-time domain derivation: the class's domain comes from the
  // card via the injected resolver (live board), NOT from the event itself.
  it('derives domain from the card via the resolver, not the event', () => {
    const rows: Array<[string, Record<string, unknown>]> = [
      ['3000000000000000020', { role: 'silas', event: 'deploy.rolledback', reason: 'x', card_id: 1320 }],
    ];
    const domainOf = (id: string): string | undefined => ({ '1320': 'photos' }[id]);
    const { classes } = rollupRawLines(lokiBody(rows), undefined, domainOf);
    expect(classes[0].domain).toBe('photos'); // card 1320's live domain, not stamped on the event
  });
  it('domain is empty with no resolver or unknown card', () => {
    const rows: Array<[string, Record<string, unknown>]> = [
      ['3000000000000000021', { role: 'silas', event: 'x.failed', reason: 'y', card_id: 9999 }],
    ];
    expect(rollupRawLines(lokiBody(rows)).classes[0].domain).toBe('');                       // no resolver
    expect(rollupRawLines(lokiBody(rows), undefined, () => undefined).classes[0].domain).toBe(''); // unknown card
  });
});
