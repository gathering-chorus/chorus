/**
 * chorus-rcas handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusRcas, type ChorusRcasDeps } from '../../src/handlers/chorus-rcas';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE rcas (
      id INTEGER PRIMARY KEY,
      title TEXT, trigger_event TEXT, timeline TEXT, root_cause TEXT,
      contributing_factors TEXT, corrective_actions TEXT,
      cards TEXT, spine_events TEXT,
      status TEXT, created_at TEXT, updated_at TEXT
    );
  `);
  const r = db.prepare(
    `INSERT INTO rcas (id, title, trigger_event, timeline, root_cause,
       contributing_factors, corrective_actions, cards, spine_events,
       status, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)`,
  );
  r.run(1, 'first rca', 'deploy-failed', 'T', 'cause', '["x"]', '["y"]', '[1,2]', '[]', 'open', '2026-04-18T10:00Z', '2026-04-18T10:00Z');
  r.run(2, 'second rca', 'trigger2', 'T2', 'cause2', '[]', '[]', '[]', '[]', 'verified', '2026-04-18T11:00Z', '2026-04-18T11:00Z');
  r.run(3, 'third rca', 'trigger3', 'T3', 'cause3', '[]', '[]', '[]', '[]', 'closed', '2026-04-18T09:00Z', '2026-04-18T09:00Z');
  return db;
}

function deps(db: Database.Database): ChorusRcasDeps {
  return { db };
}

describe('fetchChorusRcas (#2188)', () => {
  test('no filter → all rows, DESC created_at', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), {}).body as { results: Array<{ id: number }>; total: number };
    expect(body.total).toBe(3);
    expect(body.results.map((r) => r.id)).toEqual([2, 1, 3]);
    db.close();
  });

  test('status=open filters to open rows', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), { status: 'open' }).body as { results: Array<{ status: string }> };
    expect(body.results.length).toBe(1);
    expect(body.results[0].status).toBe('open');
    db.close();
  });

  test('status=verified filters', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), { status: 'verified' }).body as { results: unknown[] };
    expect(body.results.length).toBe(1);
    db.close();
  });

  test('status=closed filters', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), { status: 'closed' }).body as { results: unknown[] };
    expect(body.results.length).toBe(1);
    db.close();
  });

  test('invalid status silently ignored (returns all)', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), { status: 'INVALID' }).body as { results: unknown[] };
    expect(body.results.length).toBe(3);
    db.close();
  });

  test('trigger_event renamed to trigger', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), {}).body as { results: Array<{ trigger: string }> };
    expect(body.results[0].trigger).toBeDefined();
    expect((body.results[0] as { trigger_event?: unknown }).trigger_event).toBeUndefined();
    db.close();
  });

  test('JSON columns parsed', () => {
    const db = seedDb();
    const body = fetchChorusRcas(deps(db), { status: 'open' }).body as {
      results: Array<{ contributing_factors: unknown; corrective_actions: unknown; cards: unknown; spine_events: unknown }>;
    };
    expect(body.results[0].contributing_factors).toEqual(['x']);
    expect(body.results[0].corrective_actions).toEqual(['y']);
    expect(body.results[0].cards).toEqual([1, 2]);
    expect(body.results[0].spine_events).toEqual([]);
    db.close();
  });

  test('empty table → total 0', () => {
    const db = new Database(':memory:');
    db.exec(`CREATE TABLE rcas (id INTEGER, title TEXT, trigger_event TEXT, timeline TEXT, root_cause TEXT, contributing_factors TEXT, corrective_actions TEXT, cards TEXT, spine_events TEXT, status TEXT, created_at TEXT, updated_at TEXT);`);
    const body = fetchChorusRcas(deps(db), {}).body as { results: unknown[]; total: number };
    expect(body.total).toBe(0);
    expect(body.results).toEqual([]);
    db.close();
  });
});
