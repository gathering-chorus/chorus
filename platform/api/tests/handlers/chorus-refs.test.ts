/**
 * chorus-refs handler — unit tests (#2188).
 *
 * Seeds an in-memory SQLite DB with refs + messages, exercises filter shapes
 * and result formatting. No server boot, no network, hermetic.
 */
import Database from 'better-sqlite3';
import { fetchChorusRefs, type ChorusRefsDeps } from '../../src/handlers/chorus-refs';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      content TEXT,
      timestamp TEXT,
      role TEXT,
      source TEXT,
      channel TEXT
    );
    CREATE TABLE refs (
      entity_type TEXT,
      entity_id TEXT,
      relationship TEXT,
      message_id INTEGER REFERENCES messages(id)
    );
  `);

  const msg = db.prepare('INSERT INTO messages (id, content, timestamp, role, source, channel) VALUES (?, ?, ?, ?, ?, ?)');
  msg.run(1, 'first message about card 30', '2026-04-18T10:00:00Z', 'wren', 'slack', '#team');
  msg.run(2, 'second message about card 30', '2026-04-18T11:00:00Z', 'silas', 'claude', 'session:silas');
  msg.run(3, 'workflow 4 discussion', '2026-04-18T09:00:00Z', 'kade', 'slack', '#team');

  const ref = db.prepare('INSERT INTO refs (entity_type, entity_id, relationship, message_id) VALUES (?, ?, ?, ?)');
  ref.run('card', '#30', 'mentions', 1);
  ref.run('card', '#30', 'mentions', 2);
  ref.run('workflow', 'WF-004', 'mentions', 3);

  return db;
}

function deps(db: Database.Database): ChorusRefsDeps {
  return { db };
}

describe('fetchChorusRefs (#2188)', () => {
  test('no filters → 400', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), {});
    expect(r.status).toBe(400);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/at least one filter/i);
    db.close();
  });

  test('card filter normalizes # prefix on both directions', () => {
    const db = seedDb();
    const a = fetchChorusRefs(deps(db), { card: '30' });
    const b = fetchChorusRefs(deps(db), { card: '#30' });
    expect(a.status).toBe(200);
    expect(b.status).toBe(200);
    const bodyA = a.body as { refs: unknown[] };
    const bodyB = b.body as { refs: unknown[] };
    expect(bodyA.refs.length).toBe(2);
    expect(bodyB.refs.length).toBe(2);
    db.close();
  });

  test('wf filter adds WF- prefix when missing', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), { wf: '004' });
    expect(r.status).toBe(200);
    const body = r.body as { refs: Array<{ entity_id: string }> };
    expect(body.refs.length).toBe(1);
    expect(body.refs[0].entity_id).toBe('WF-004');
    db.close();
  });

  test('type + entityId filter combined', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), { type: 'workflow', entityId: 'WF-004' });
    const body = r.body as { refs: unknown[] };
    expect(body.refs.length).toBe(1);
    db.close();
  });

  test('card filter takes precedence over type+entityId', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), { card: '30', type: 'workflow' });
    const body = r.body as { refs: unknown[] };
    expect(body.refs.length).toBe(2);
    db.close();
  });

  test('results ordered by timestamp DESC', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), { card: '30' });
    const body = r.body as { refs: Array<{ message: { timestamp: string } }> };
    expect(body.refs[0].message.timestamp).toBe('2026-04-18T11:00:00Z');
    expect(body.refs[1].message.timestamp).toBe('2026-04-18T10:00:00Z');
    db.close();
  });

  test('message.content truncated to 500 chars', () => {
    const db = seedDb();
    const long = 'x'.repeat(1000);
    db.prepare('INSERT INTO messages (id, content, timestamp, role, source, channel) VALUES (?, ?, ?, ?, ?, ?)')
      .run(5, long, '2026-04-18T12:00:00Z', 'wren', 'slack', '#team');
    db.prepare('INSERT INTO refs (entity_type, entity_id, relationship, message_id) VALUES (?, ?, ?, ?)')
      .run('card', '#99', 'mentions', 5);
    const r = fetchChorusRefs(deps(db), { card: '99' });
    const body = r.body as { refs: Array<{ message: { content: string } }> };
    expect(body.refs[0].message.content.length).toBe(500);
    db.close();
  });

  test('null message.content survives (?. operator returns undefined)', () => {
    const db = seedDb();
    db.prepare('INSERT INTO messages (id, content, timestamp, role, source, channel) VALUES (?, ?, ?, ?, ?, ?)')
      .run(6, null, '2026-04-18T13:00:00Z', 'wren', 'slack', '#team');
    db.prepare('INSERT INTO refs (entity_type, entity_id, relationship, message_id) VALUES (?, ?, ?, ?)')
      .run('card', '#77', 'mentions', 6);
    const r = fetchChorusRefs(deps(db), { card: '77' });
    const body = r.body as { refs: Array<{ message: { content: string | undefined } }> };
    expect(body.refs[0].message.content).toBeUndefined();
    db.close();
  });

  test('no matching refs → 200 with empty array', () => {
    const db = seedDb();
    const r = fetchChorusRefs(deps(db), { card: 'nonexistent' });
    expect(r.status).toBe(200);
    const body = r.body as { refs: unknown[] };
    expect(body.refs).toEqual([]);
    db.close();
  });
});
