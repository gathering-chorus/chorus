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
  test('returns hops for correlation id, ordered by hop ASC', () => {
    const db = seedDb();
    const r = fetchTraceByCorrelation('C1', { db });
    expect(r.status).toBe(200);
    expect(r.body.correlationId).toBe('C1');
    const hops = r.body.hops as Array<{ hop: number; source_service: string }>;
    expect(hops).toHaveLength(3);
    expect(hops.map((h) => h.hop)).toEqual([0, 1, 2]);
    expect(hops[0].source_service).toBe('music-api');
    db.close();
  });

  test('unknown correlation id returns 200 with empty hops', () => {
    const db = seedDb();
    const r = fetchTraceByCorrelation('NONE', { db });
    expect(r.status).toBe(200);
    expect(r.body.hops).toEqual([]);
    db.close();
  });

  test('filters strictly by correlation id', () => {
    const db = seedDb();
    const r = fetchTraceByCorrelation('C2', { db });
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
