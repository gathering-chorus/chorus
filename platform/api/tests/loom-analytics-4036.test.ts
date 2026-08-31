// @test-type: unit — hermetic: computeLoomAnalytics on fixture rows, no DB
// #4036 — the migrated #2116 instrument. The negative proofs matter: a done
// card OUTSIDE the range must not count (range filter real), and an unowned
// done card must not mint a role row (no phantom roles).
import { computeLoomAnalytics, fetchLoomAnalytics, LoomCardRow } from '../src/handlers/loom-analytics';

const NOW = Date.parse('2026-08-31T16:00:00Z');
const iso = (daysAgo: number) => new Date(NOW - daysAgo * 24 * 3600 * 1000).toISOString();

const row = (o: Partial<LoomCardRow>): LoomCardRow => ({
  id: 1, status: 'Next', owner: '', created: iso(10), done: false, doneAt: '', ...o,
});

const FIX: LoomCardRow[] = [
  row({ id: 1, status: 'Done', owner: 'wren', created: iso(5), done: true, doneAt: iso(2) }),
  row({ id: 2, status: 'Done', owner: 'wren', created: iso(9), done: true, doneAt: iso(3) }),
  row({ id: 3, status: 'Done', owner: 'kade', created: iso(60), done: true, doneAt: iso(40) }), // outside 7d/30d
  row({ id: 4, status: 'Done', owner: '', created: iso(4), done: true, doneAt: iso(1) }),       // unowned
  row({ id: 5, status: 'WIP', owner: 'silas' }),
  row({ id: 6, status: 'Next' }),
  row({ id: 7, status: 'Next' }),
];

describe('computeLoomAnalytics (#4036, migrated #2116)', () => {
  it('cumulative flow counts every card by status', () => {
    const m = computeLoomAnalytics(FIX, 'all', NOW);
    expect(m.flow).toEqual({ Done: 4, WIP: 1, Next: 2 });
    expect(m.totalCards).toBe(7);
  });

  it('range filter EXCLUDES a done card outside the window (negative proof)', () => {
    const m7 = computeLoomAnalytics(FIX, '7d', NOW);
    expect(m7.throughput).toBe(3);           // ids 1,2,4 — NOT the 40-day-old #3
    const all = computeLoomAnalytics(FIX, 'all', NOW);
    expect(all.throughput).toBe(4);          // #3 counts only here
  });

  it('roleFitness: lead times per role; unowned shipped cards mint NO role row (negative proof)', () => {
    const m = computeLoomAnalytics(FIX, '7d', NOW) as { roleFitness: Array<{ role: string; shipped: number; avgLeadHours: number; maxLeadHours: number }> };
    expect(m.roleFitness.map((r) => r.role)).toEqual(['wren']); // #4 unowned, #3 out of range
    expect(m.roleFitness[0].shipped).toBe(2);
    expect(m.roleFitness[0].avgLeadHours).toBe(108);  // (72h + 144h) / 2
    expect(m.roleFitness[0].maxLeadHours).toBe(144);
  });

  it('daily throughput series is date-sorted with counts', () => {
    const m = computeLoomAnalytics(FIX, '7d', NOW) as { dailyThroughput: Array<{ date: string; count: number }> };
    expect(m.dailyThroughput.length).toBe(3);
    const dates = m.dailyThroughput.map((d) => d.date);
    expect([...dates].sort()).toEqual(dates);
  });

  it('bottleneck names the fattest open status; null on an all-done board', () => {
    const m = computeLoomAnalytics(FIX, 'all', NOW);
    expect(m.bottleneck).toEqual({ status: 'Next', count: 2 });
    const done = computeLoomAnalytics([FIX[0]], 'all', NOW);
    expect(done.bottleneck).toBeNull();
  });

  it('fetchLoomAnalytics: unknown range falls back to all; a throwing reader is a 500, not a lie', () => {
    const ok = fetchLoomAnalytics({ readCards: () => FIX, now: () => NOW }, 'bogus');
    expect(ok.status).toBe(200);
    expect(ok.body.range).toBe('all');
    const bad = fetchLoomAnalytics({ readCards: () => { throw new Error('db gone'); } }, '7d');
    expect(bad.status).toBe(500);
    expect(String(bad.body.error)).toContain('db gone');
  });
});
