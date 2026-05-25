/**
 * #3088 — collectSpine now reads card.* events from Loki (async, the spine superset)
 * instead of a 4MB sync chorus.log tail + JSON.parse-per-line (the ~1s loop block).
 * Behavior-preserving: same card.* filter, same cardId filter, same shape, chronological.
 */
import { collectSpine } from '../../src/handlers/chorus-crawl';
import type { FetchFn } from '../../src/handlers/chorus-crawl';

function lokiFetch(lines: string[]): FetchFn {
  return (async () => ({
    ok: true,
    status: 200,
    json: async () => ({ data: { result: [{ values: lines.map((l, i) => [String(i), l]) }] } }),
  })) as unknown as FetchFn;
}

const cards = [{ index: 3086, title: 't', status: 'Done', owner: 'wren' }];

describe('collectSpine (#3088 — Loki, off-loop)', () => {
  test('returns card.* events for the crawl cards, chronological, and fills timeline', async () => {
    const lines = [
      JSON.stringify({ event: 'card.accepted', card: '3086', role: 'wren', timestamp: '2026-05-25T12:00:00Z' }),
      JSON.stringify({ event: 'card.pulled', card: '3086', role: 'wren', timestamp: '2026-05-25T11:00:00Z' }),
      JSON.stringify({ event: 'card.pulled', card: '9999', role: 'kade', timestamp: '2026-05-25T11:30:00Z' }), // not a crawl card → dropped
      JSON.stringify({ event: 'nudge.emitted', card: '3086', role: 'wren', timestamp: '2026-05-25T11:45:00Z' }), // not card.* → dropped
    ];
    const timeline: Array<{ source: string }> = [];
    const spine = await collectSpine(lokiFetch(lines), 'http://loki', '/chorus.log', cards, timeline, () => 1_000_000);
    expect(spine).toHaveLength(2);
    expect(spine[0].timestamp).toBe('2026-05-25T11:00:00Z'); // chronological asc
    expect(spine[1].event).toBe('card.accepted');
    expect(timeline).toHaveLength(2);
    expect(timeline[0].source).toBe('spine');
  });

  test('degrades to empty when Loki is unreachable (no throw)', async () => {
    const downFetch = (async () => { throw new Error('loki down'); }) as unknown as FetchFn;
    const timeline: unknown[] = [];
    const spine = await collectSpine(downFetch, 'http://loki', '/chorus.log', cards, timeline, () => 1_000_000);
    expect(spine.length).toBe(0);
    expect(timeline.length).toBe(0);
  });

  test('returns empty when Loki responds not-ok', async () => {
    const notOk = (async () => ({ ok: false, status: 503, json: async () => ({}) })) as unknown as FetchFn;
    const spine = await collectSpine(notOk, 'http://loki', '/chorus.log', cards, [], () => 1_000_000);
    expect(spine.length).toBe(0);
  });
});
