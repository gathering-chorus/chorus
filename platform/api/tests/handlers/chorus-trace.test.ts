// @test-type: unit — in-memory sqlite + injected spine fetcher; no Loki, no live services.
/**
 * chorus-trace handler — unit tests (#2189).
 *
 * In-memory SQLite seeded with a `traces` table. Tests verify:
 *   - /trace/:correlationId returns hops ordered by hop ASC, filtered by id
 *   - missing correlation id → empty hops array (200, not 404)
 *   - /trace/integrations/:domain groups by (src, dest, call_stack), counts
 *     frequency, orders desc
 *   - rows without dest_service excluded from integrations
 *   - domain filter applied strictly
 */
import Database from 'better-sqlite3';
import {
  fetchTraceByCorrelation,
  fetchTraceIntegrations,
} from '../../src/handlers/chorus-trace';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE traces (
      correlation_id TEXT,
      hop INTEGER,
      source_service TEXT,
      source_domain TEXT,
      dest_service TEXT,
      call_stack TEXT
    );
  `);
  const ins = db.prepare(
    'INSERT INTO traces (correlation_id, hop, source_service, source_domain, dest_service, call_stack) VALUES (?, ?, ?, ?, ?, ?)',
  );
  // Trace C1: 3 hops music → photos → notes
  ins.run('C1', 0, 'music-api', 'music', 'photos-api', 'stack:A');
  ins.run('C1', 1, 'photos-api', 'photos', 'notes-api', 'stack:B');
  ins.run('C1', 2, 'notes-api', 'notes', null, null);
  // Trace C2: separate hop
  ins.run('C2', 0, 'music-api', 'music', 'search-api', 'stack:A');
  // Repeated pair for integrations count
  ins.run('C3', 0, 'music-api', 'music', 'photos-api', 'stack:A');
  ins.run('C4', 0, 'music-api', 'music', 'photos-api', 'stack:A');
  return db;
}

describe('fetchTraceByCorrelation (#2189 /api/chorus/trace/:correlationId)', () => {
  test('returns hops for correlation id, ordered by hop ASC', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('C1', { db });
    expect(r.status).toBe(200);
    expect(r.body.correlationId).toBe('C1');
    const hops = r.body.hops as Array<{ hop: number; source_service: string }>;
    expect(hops).toHaveLength(3);
    expect(hops.map((h) => h.hop)).toEqual([0, 1, 2]);
    expect(hops[0].source_service).toBe('music-api');
    db.close();
  });

  test('unknown correlation id returns 200 with empty hops', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('NONE', { db });
    expect(r.status).toBe(200);
    expect(r.body.hops).toEqual([]);
    db.close();
  });

  test('filters strictly by correlation id', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('C2', { db });
    const hops = r.body.hops as unknown[];
    expect(hops).toHaveLength(1);
    db.close();
  });
});

describe('fetchTraceIntegrations (#2189 /api/chorus/trace/integrations/:domain)', () => {
  test('groups by (source, dest, stack), counts frequency, orders desc', () => {
    const db = seedDb();
    const r = fetchTraceIntegrations('music', { db });
    expect(r.status).toBe(200);
    expect(r.body.domain).toBe('music');
    const rows = r.body.integrations as Array<{
      source_service: string;
      dest_service: string;
      frequency: number;
    }>;
    expect(rows[0]).toMatchObject({
      source_service: 'music-api',
      dest_service: 'photos-api',
      frequency: 3,
    });
    expect(rows[1]).toMatchObject({
      source_service: 'music-api',
      dest_service: 'search-api',
      frequency: 1,
    });
    db.close();
  });

  test('excludes rows with null dest_service', () => {
    const db = seedDb();
    const r = fetchTraceIntegrations('notes', { db });
    expect(r.body.integrations).toEqual([]);
    db.close();
  });

  test('strict domain filter', () => {
    const db = seedDb();
    const r = fetchTraceIntegrations('photos', { db });
    const rows = r.body.integrations as unknown[];
    expect(rows).toHaveLength(1);
    db.close();
  });
});

// --- #3621 — trace visibility: the viewer must fold SPINE events by trace_id,
// not just the hop table. Jeff's #3609 ask: GET /api/chorus/trace/:id returned
// {hops:[]} while Loki held 224 events with that exact trace_id. The fold makes
// the built viewer show the run; hops stay for back-compat (secondary source).
describe('fetchTraceByCorrelation — spine fold (#3621)', () => {
  const spineRows = [
    { ts: '2026-07-04T14:58:07.858Z', event: 'pull.completed', role: 'wren', card_id: 3609, branch: 'wren/3609' },
    { ts: '2026-07-04T17:02:00.355Z', event: 'commit.completed', role: 'wren', card_id: 3609, sha: 'dfb44215' },
    { ts: '2026-07-04T17:04:53.826Z', event: 'build.completed', role: 'wren', card_id: 3609 },
  ];

  test('folds spine events from the injected fetcher into events + a merged timeline', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('C1', {
      db,
      fetchSpineEvents: async (id) => (id === 'C1' ? spineRows : []),
    });
    expect(r.status).toBe(200);
    expect(r.body.events).toHaveLength(3);
    expect((r.body.events as Array<{ event: string }>)[0].event).toBe('pull.completed');
    // timeline = hops + events, one chronological stream; hops keep their rows
    expect(r.body.hops).toHaveLength(3);
    const timeline = r.body.timeline as Array<{ kind: string }>;
    expect(timeline.filter((t) => t.kind === 'spine')).toHaveLength(3);
    expect(timeline.filter((t) => t.kind === 'hop')).toHaveLength(3);
    db.close();
  });

  test('spine events are chronological in the timeline', async () => {
    const db = seedDb();
    const shuffled = [spineRows[2], spineRows[0], spineRows[1]];
    const r = await fetchTraceByCorrelation('C1', { db, fetchSpineEvents: async () => shuffled });
    const spine = (r.body.timeline as Array<{ kind: string; event?: string }>).filter((t) => t.kind === 'spine');
    expect(spine.map((s) => s.event)).toEqual(['pull.completed', 'commit.completed', 'build.completed']);
    db.close();
  });

  test('without a fetcher the old shape holds (hops only, events empty)', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('C1', { db });
    expect(r.body.hops).toHaveLength(3);
    expect(r.body.events).toEqual([]);
    db.close();
  });

  test('a failing fetcher degrades to hops-only with spine_error noted — never throws', async () => {
    const db = seedDb();
    const r = await fetchTraceByCorrelation('C1', {
      db,
      fetchSpineEvents: async () => { throw new Error('loki down'); },
    });
    expect(r.status).toBe(200);
    expect(r.body.events).toEqual([]);
    expect(r.body.spine_error).toContain('loki down');
    db.close();
  });
});
