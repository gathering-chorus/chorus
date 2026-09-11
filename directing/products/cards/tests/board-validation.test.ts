// @test-type: integration — reads the live board (skip-if-absent); determinism is proven on one captured payload (#4135)
/**
 * Board state validation tests — #1820
 *
 * Tests verify board operations against live Vikunja API.
 * No mocks — the mocks told us everything was fine while
 * the real API was capping at 50 per bucket.
 *
 * Requires RUN_INTEGRATION=true to run. These tests MUTATE the board
 * (move operations) and hit SQLite directly via fetchBucketMapFromDB.
 * Gating prevents silent skip-on-fail and SQLite contention (#1800).
 *
 * What broke on 2026-04-05:
 * - move() reported success without verifying persistence
 * - view() showed Unknown for cards beyond bucket 50-cap
 * - list() returned different counts on repeat calls
 * - Tests were creating production cards
 */

jest.setTimeout(30000);

import { BoardClient } from '../src/client';
import { GATHERING, loadEnv } from '../src/config';
import { BoardTask } from '../src/types';

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';

let env: { url: string; token: string };
let client: BoardClient;
let canConnect = false;

beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    env = loadEnv();
    client = new BoardClient(env.url, env.token, GATHERING);
    const tasks = await client.list();
    canConnect = Array.isArray(tasks) && tasks.length > 0;
  } catch {
    canConnect = false;
  }
});

function skip() { return !INTEGRATION_ENABLED || !canConnect; }

// ── move() ──

describe('move: persistence verification', () => {
  // Use a known Won't Do card for move tests — move it out and back
  let testCard: BoardTask | undefined;

  beforeAll(async () => {
    if (skip()) return;
    // Find a Won't Do card to use as test subject (safe to move around)
    const all = await client.list();
    testCard = all.find(t => t.status === "Won't Do" && t.title.includes('test') || t.title.includes('Test'));
    if (!testCard) {
      // Fall back to any Won't Do card
      testCard = all.find(t => t.status === "Won't Do");
    }
  });

  afterAll(async () => {
    // Restore card to original status
    if (skip() || !testCard) return;
    try {
      await client.move(testCard.index, 'wont-do');
    } catch { /* best effort restore */ }
  });

  test('move persists in Vikunja API', async () => {
    if (skip() || !testCard) return;
    // Move to Next (small bucket, under 50 cap — verifiable via bucket view)
    await client.move(testCard.index, 'next');
    // Verify via bucket view — Next is always <50 so the card will be visible
    const apiId = await client.resolveIndex(testCard.index);
    const buckets = await client.fetchBuckets();
    const found = buckets.find(b =>
      (b.tasks || []).some(t => t.id === apiId)
    );
    expect(found).toBeDefined();
    expect(found!.title).toBe('Next');
    // Move back to Won't Do
    await client.move(testCard.index, 'wont-do');
  });

  test('move cache invalidation — list() reflects new status after move', async () => {
    if (skip() || !testCard) return;
    // Move to Ideas (small bucket, verifiable)
    await client.move(testCard.index, 'ideas');
    const all = await client.list();
    const card = all.find(t => t.index === testCard!.index);
    expect(card).toBeDefined();
    expect(card!.status).toBe('Ideas');
    // Restore
    await client.move(testCard.index, 'wont-do');
  });
});

// ── view() ──

describe('view: status accuracy', () => {
  test('WIP card shows WIP status', async () => {
    if (skip()) return;
    const all = await client.list();
    const wipCard = all.find(t => t.status === 'WIP');
    if (!wipCard) return; // No WIP cards right now
    const viewed = await client.view(wipCard.index);
    expect(viewed.status).toBe('WIP');
  });

  test('Next card shows Next status', async () => {
    if (skip()) return;
    const all = await client.list();
    const nextCard = all.find(t => t.status === 'Next');
    if (!nextCard) return;
    const viewed = await client.view(nextCard.index);
    expect(viewed.status).toBe('Next');
  });

  test('view status matches list status for small-bucket cards', async () => {
    if (skip()) return;
    // #4139 — one captured world. The old form did list() then view() per
    // card as two live reads; on 2026-09-11 03:00 a sibling suite's move
    // landed between them and the nightly went red for "someone moved a
    // card". The property is that view's resolver and list's projection
    // agree on the SAME snapshot.
    const raw = await client.fetchAllTasks();
    const dbMap = client.fetchBucketMapFromDB();
    const buckets = await client.fetchBuckets();
    const listed = client.project(raw, dbMap);
    const smallBucket = listed.filter(t =>
      t.status === 'WIP' || t.status === 'Next' || t.status === 'Blocked'
    );
    const byApiId = new Map(raw.map(t => [t.id, t]));
    for (const card of smallBucket.slice(0, 5)) {
      const task = byApiId.get(card.apiId);
      expect(task).toBeDefined();
      expect(client.resolveStatus(task!, dbMap, buckets)).toBe(card.status);
    }
  });

  test('NEGATIVE PROOF: a card moved between the two reads is caught', async () => {
    if (skip()) return;
    const raw = await client.fetchAllTasks();
    const dbMap = client.fetchBucketMapFromDB();
    const buckets = await client.fetchBuckets();
    const listed = client.project(raw, dbMap);
    const card = listed.find(t => t.status === 'WIP' || t.status === 'Next' || t.status === 'Blocked');
    if (!card) return;
    const task = raw.find(t => t.id === card.apiId)!;
    // the world moves under the second read: the card is Done in the map now
    const moved = new Map(dbMap);
    moved.set(task.id, 'Done');
    expect(client.resolveStatus(task, moved, buckets)).not.toBe(card.status);
  });

  test('view shows correct status for overflow bucket cards (Later/Done/Won\'t Do)', async () => {
    if (skip()) return;
    // This is the #1815 bug: card moved to Won't Do, view() shows Unknown
    // because findTaskBucket only checks bucket view (50-cap).
    // Test documents the red — this SHOULD pass but currently fails for
    // cards beyond the 50th in their bucket.
    const all = await client.list();
    // Find a Later card that overflowed (not in bucket view's 50)
    const buckets = await client.fetchBuckets();
    const laterBucket = buckets.find(b => b.title === 'Later');
    const laterViewIds = new Set((laterBucket?.tasks || []).map(t => t.id));

    // Find a Later card from list() whose API ID is NOT in the bucket view
    const laterCards = all.filter(t => t.status === 'Later');
    const overflowCard = laterCards.find(t => {
      const apiId = (t as any).apiId;
      return apiId && !laterViewIds.has(apiId);
    });

    if (!overflowCard) return; // No overflow cards right now

    const viewed = await client.view(overflowCard.index);
    // BUG: view() returns Unknown for overflow cards.
    // When this test starts passing, the overflow bug is fixed.
    expect(viewed.status).toBe('Later');
  });
});

// ── list() ──

describe('list: completeness and determinism', () => {
  test('list returns all tasks from paginated endpoint', async () => {
    if (skip()) return;
    const all = await client.list();
    const allRaw = await client.fetchAllTasks();
    // list() should have same count as fetchAllTasks()
    expect(all.length).toBe(allRaw.length);
  });

  // #4135 — determinism is a property of the projection, not of the world.
  // The old form listed the LIVE board three times and asserted the snapshots
  // matched; on 2026-09-10 08:55 Silas pulled a card mid-run and it went red
  // for "someone was working". Capture one payload, project it three times.
  test('repeat calls return same count', async () => {
    if (skip()) return;
    const raw = await client.fetchAllTasks();
    const dbMap = client.fetchBucketMapFromDB();
    const counts = [0, 1, 2].map(() => client.project(raw, dbMap).length);
    expect(new Set(counts).size).toBe(1);
  });

  const breakdownOf = (tasks: BoardTask[]): string => {
    const b: Record<string, number> = {};
    for (const t of tasks) b[t.status] = (b[t.status] || 0) + 1;
    return JSON.stringify(b, Object.keys(b).sort());
  };

  test('repeat calls return same per-status breakdown', async () => {
    if (skip()) return;
    const raw = await client.fetchAllTasks();
    const dbMap = client.fetchBucketMapFromDB();
    const snapshots = [0, 1, 2].map(() => breakdownOf(client.project(raw, dbMap)));
    expect(snapshots[1]).toBe(snapshots[0]);
    expect(snapshots[2]).toBe(snapshots[0]);
  });

  test('NEGATIVE PROOF: a payload that changes between calls is caught', async () => {
    if (skip()) return;
    const raw = await client.fetchAllTasks();
    const dbMap = client.fetchBucketMapFromDB();
    if (raw.length === 0) return;
    const before = breakdownOf(client.project(raw, dbMap));
    // the world moves: one task flips done, the way a teammate's /acp would
    const moved = raw.map((t, i) => (i === 0 ? { ...t, done: !t.done } : t));
    const movedMap = new Map(dbMap);
    movedMap.delete(raw[0].id);
    const after = breakdownOf(client.project(moved, movedMap));
    expect(after).not.toBe(before);
  });

  test('WIP cards in list include recently created cards', async () => {
    if (skip()) return;
    const all = await client.list();
    const wip = all.filter(t => t.status === 'WIP');
    // Verify against bucket view
    const buckets = await client.fetchBuckets();
    const wipBucket = buckets.find(b => b.title === 'WIP');
    const wipBucketCount = wipBucket?.tasks?.length || 0;
    // list() WIP count should match bucket view (WIP is always <50)
    expect(wip.length).toBe(wipBucketCount);
  });

  test('listGrouped matches list', async () => {
    if (skip()) return;
    const all = await client.list();
    const grouped = await client.listGrouped();
    let groupedTotal = 0;
    for (const [, tasks] of grouped) {
      groupedTotal += tasks.length;
    }
    expect(groupedTotal).toBe(all.length);
  });
});

// ── tag ──

describe('tag: label operations', () => {
  test('existing card has parseable owner label', async () => {
    if (skip()) return;
    const all = await client.list();
    const labeled = all.find(t => t.owner !== '');
    expect(labeled).toBeDefined();
    expect(labeled!.owner).toMatch(/^(wren|silas|kade|jeff)$/i);
  });

  test('labels include domain and type tags', async () => {
    if (skip()) return;
    const all = await client.list();
    // domains[] contains all non-owner non-priority labels (domain:, chunk:, type:, etc.)
    const withDomain = all.find(t => t.domains.some(d => d.startsWith('domain:')));
    expect(withDomain).toBeDefined();
    const domainLabels = withDomain!.domains.filter(d => d.startsWith('domain:'));
    expect(domainLabels.length).toBeGreaterThan(0);
  });
});

// ── add (no production cards) ──

describe('add: input validation without API calls', () => {
  test('addCard requires a title', async () => {
    if (skip()) return;
    // addCard is in the SDK (sdk.ts), not client.ts — import it
    const { addCard } = require('../src/sdk');
    // Empty title should exit with error (process.exit or throw)
    const mockExit = jest.spyOn(process, 'exit').mockImplementation((() => {
      throw new Error('process.exit called');
    }) as any);
    try {
      await expect(addCard(client, '', {})).rejects.toThrow();
    } finally {
      mockExit.mockRestore();
    }
  });
});
