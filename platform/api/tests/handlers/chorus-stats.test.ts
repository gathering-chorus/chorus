/**
 * chorus-stats handler — unit tests (#2188).
 *
 * In-memory SQLite, seeded with messages + watermarks + refs, tests assert
 * each aggregate shape.
 */
import Database from 'better-sqlite3';
import { fetchChorusStats, type ChorusStatsDeps } from '../../src/handlers/chorus-stats';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      source TEXT,
      role TEXT,
      timestamp TEXT
    );
    CREATE TABLE watermarks (
      source TEXT PRIMARY KEY,
      last_indexed TEXT
    );
    CREATE TABLE refs (
      entity_id TEXT
    );
  `);
  const m = db.prepare('INSERT INTO messages (source, role, timestamp) VALUES (?, ?, ?)');
  m.run('slack', 'wren', '2026-04-18T10:00:00Z');
  m.run('slack', 'silas', '2026-04-18T11:00:00Z');
  m.run('slack', 'kade', '2026-04-18T09:00:00Z');
  m.run('claude', 'wren', '2026-04-18T08:00:00Z');
  m.run('spine', 'silas', '2026-04-17T23:00:00Z');

  const w = db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)');
  w.run('slack:team', '2026-04-18T11:00:00Z');
  w.run('claude:wren', '2026-04-18T10:00:00Z');
  w.run('spine', '2026-04-17T23:00:00Z');

  const r = db.prepare('INSERT INTO refs (entity_id) VALUES (?)');
  r.run('#30');
  r.run('#31');
  return db;
}

function deps(db: Database.Database): ChorusStatsDeps {
  return { db };
}

describe('fetchChorusStats (#2188)', () => {
  test('total counts all message rows', () => {
    const db = seedDb();
    const r = fetchChorusStats(deps(db));
    expect(r.status).toBe(200);
    const body = r.body as { total: number };
    expect(body.total).toBe(5);
    db.close();
  });

  test('bySource groups and orders DESC by count', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { bySource: Record<string, number> };
    expect(body.bySource).toEqual({ slack: 3, claude: 1, spine: 1 });
    // Insertion order preserved in JS object ≈ order of bySource rows by count DESC
    expect(Object.keys(body.bySource)[0]).toBe('slack');
    db.close();
  });

  test('byRole groups by role', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { byRole: Record<string, number> };
    expect(body.byRole).toEqual({ wren: 2, silas: 2, kade: 1 });
    db.close();
  });

  test('dateRange returns min/max timestamps', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { dateRange: { earliest: string; latest: string } };
    expect(body.dateRange.earliest).toBe('2026-04-17T23:00:00Z');
    expect(body.dateRange.latest).toBe('2026-04-18T11:00:00Z');
    db.close();
  });

  test('lastIndexed is most-recent watermark', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { lastIndexed: string };
    expect(body.lastIndexed).toBe('2026-04-18T11:00:00Z');
    db.close();
  });

  test('watermarks returned ordered DESC by last_indexed', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { watermarks: Array<{ last_indexed: string }> };
    expect(body.watermarks.map((w) => w.last_indexed)).toEqual([
      '2026-04-18T11:00:00Z',
      '2026-04-18T10:00:00Z',
      '2026-04-17T23:00:00Z',
    ]);
    db.close();
  });

  test('refs counts refs table rows', () => {
    const db = seedDb();
    const body = fetchChorusStats(deps(db)).body as { refs: number };
    expect(body.refs).toBe(2);
    db.close();
  });

  test('empty DB returns zero counts + null dates', () => {
    const db = new Database(':memory:');
    db.exec(`
      CREATE TABLE messages (id INTEGER, source TEXT, role TEXT, timestamp TEXT);
      CREATE TABLE watermarks (source TEXT, last_indexed TEXT);
      CREATE TABLE refs (entity_id TEXT);
    `);
    const body = fetchChorusStats(deps(db)).body as {
      total: number; refs: number; dateRange: { earliest: string | null; latest: string | null }; lastIndexed: string | null; watermarks: unknown[];
    };
    expect(body.total).toBe(0);
    expect(body.refs).toBe(0);
    expect(body.dateRange.earliest).toBeNull();
    expect(body.dateRange.latest).toBeNull();
    expect(body.lastIndexed).toBeNull();
    expect(body.watermarks).toEqual([]);
    db.close();
  });
});
