/**
 * chorus-reconcile handler — unit tests (#2188).
 *
 * In-memory SQLite, deterministic now, hermetic.
 */
import Database from 'better-sqlite3';
import { fetchChorusReconcile, type ChorusReconcileDeps } from '../../src/handlers/chorus-reconcile';

const FIXED_NOW = new Date('2026-04-18T12:00:00Z').getTime();
const TWENTY_FIVE_HOURS_AGO = new Date(FIXED_NOW - 25 * 60 * 60 * 1000).toISOString();
const TWENTY_THREE_HOURS_AGO = new Date(FIXED_NOW - 23 * 60 * 60 * 1000).toISOString();
const ONE_HOUR_AGO = new Date(FIXED_NOW - 60 * 60 * 1000).toISOString();
const TWO_HOURS_AGO = new Date(FIXED_NOW - 2 * 60 * 60 * 1000).toISOString();

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      source TEXT,
      channel TEXT,
      role TEXT,
      author TEXT,
      content TEXT,
      timestamp TEXT,
      is_bridge INTEGER DEFAULT 0
    );
  `);
  return db;
}

function addMsg(
  db: Database.Database,
  m: {
    source: string; channel: string; role: string; author: string; content: string;
    timestamp: string; is_bridge?: number;
  },
) {
  db.prepare(
    'INSERT INTO messages (source, channel, role, author, content, timestamp, is_bridge) VALUES (?,?,?,?,?,?,?)',
  ).run(m.source, m.channel, m.role, m.author, m.content, m.timestamp, m.is_bridge ?? 0);
}

function deps(db: Database.Database): ChorusReconcileDeps {
  return { db, now: () => FIXED_NOW };
}

describe('fetchChorusReconcile (#2188)', () => {
  test('missing role → 400', () => {
    const db = seedDb();
    const r = fetchChorusReconcile(deps(db), {});
    expect(r.status).toBe(400);
    db.close();
  });

  test('unknown role → 400', () => {
    const db = seedDb();
    const r = fetchChorusReconcile(deps(db), { role: 'jeff' });
    expect(r.status).toBe(400);
    db.close();
  });

  test('accepts wren|silas|kade', () => {
    const db = seedDb();
    for (const role of ['wren', 'silas', 'kade']) {
      expect(fetchChorusReconcile(deps(db), { role }).status).toBe(200);
    }
    db.close();
  });

  test('no prior session → cutoff falls back to 24h before now', () => {
    const db = seedDb();
    // Message 25h ago (outside), and 23h ago (inside 24h window)
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'old', timestamp: TWENTY_FIVE_HOURS_AGO });
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'recent', timestamp: TWENTY_THREE_HOURS_AGO });
    const r = fetchChorusReconcile(deps(db), { role: 'wren' });
    const body = r.body as { slack: Record<string, Array<{ content: string }>> };
    expect(body.slack['#team']).toEqual([{ channel: '#team', role: 'silas', author: 'silas', content: 'recent', timestamp: TWENTY_THREE_HOURS_AGO }]);
    db.close();
  });

  test('prior session sets cutoff; slack since then appears', () => {
    const db = seedDb();
    // Wren's session ended TWO_HOURS_AGO
    addMsg(db, { source: 'claude', channel: 'session:wren', role: 'wren', author: 'assistant', content: 'end', timestamp: TWO_HOURS_AGO });
    // Slack message BEFORE session end → excluded
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'before', timestamp: new Date(FIXED_NOW - 3 * 60 * 60 * 1000).toISOString() });
    // Slack message AFTER session end → included
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'after', timestamp: ONE_HOUR_AGO });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { slack: Record<string, Array<{ content: string }>> };
    expect(body.slack['#team'].map((m) => m.content)).toEqual(['after']);
    db.close();
  });

  test('slack grouped by channel, max 5 per channel', () => {
    const db = seedDb();
    for (let i = 0; i < 8; i++) {
      addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: `m${i}`, timestamp: new Date(FIXED_NOW - (8 - i) * 60000).toISOString() });
    }
    addMsg(db, { source: 'slack', channel: '#random', role: 'kade', author: 'kade', content: 'r', timestamp: ONE_HOUR_AGO });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { slack: Record<string, unknown[]> };
    expect(body.slack['#team'].length).toBe(5);
    expect(body.slack['#random'].length).toBe(1);
    db.close();
  });

  test('bridged messages from the same role are filtered out', () => {
    const db = seedDb();
    addMsg(db, { source: 'slack', channel: '#team', role: 'wren', author: 'wren', content: 'bridged', timestamp: ONE_HOUR_AGO, is_bridge: 1 });
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'other', timestamp: ONE_HOUR_AGO });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { slack: Record<string, Array<{ content: string }>> };
    expect(body.slack['#team'].map((m) => m.content)).toEqual(['other']);
    db.close();
  });

  test('bridged messages from OTHER roles are kept', () => {
    const db = seedDb();
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'bridged-by-other', timestamp: ONE_HOUR_AGO, is_bridge: 1 });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { slack: Record<string, Array<{ content: string }>> };
    expect(body.slack['#team'].map((m) => m.content)).toEqual(['bridged-by-other']);
    db.close();
  });

  test('sessions count other roles messages since cutoff', () => {
    const db = seedDb();
    // Wren's session ends at TWO_HOURS_AGO; others' messages at ONE_HOUR_AGO are strictly after.
    addMsg(db, { source: 'claude', channel: 'session:wren', role: 'wren', author: 'assistant', content: 'self-cutoff', timestamp: TWO_HOURS_AGO });
    addMsg(db, { source: 'claude', channel: 'session:silas', role: 'silas', author: 'assistant', content: 'a', timestamp: ONE_HOUR_AGO });
    addMsg(db, { source: 'claude', channel: 'session:silas', role: 'silas', author: 'assistant', content: 'b', timestamp: ONE_HOUR_AGO });
    addMsg(db, { source: 'claude', channel: 'session:kade', role: 'kade', author: 'assistant', content: 'c', timestamp: ONE_HOUR_AGO });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { sessions: Record<string, number> };
    expect(body.sessions).toEqual({ silas: 2, kade: 1 });
    db.close();
  });

  test('jeffDirection returns user-authored messages, DESC timestamp, limit 10', () => {
    const db = seedDb();
    for (let i = 0; i < 12; i++) {
      addMsg(db, {
        source: 'claude', channel: 'session:kade', role: 'kade', author: 'user',
        content: `jeff-${i}`, timestamp: new Date(FIXED_NOW - (12 - i) * 60000).toISOString(),
      });
    }
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { jeffDirection: Array<{ content: string }> };
    expect(body.jeffDirection.length).toBe(10);
    expect(body.jeffDirection[0].content).toBe('jeff-11');
  });

  test('stats contains total + bySource aggregate', () => {
    const db = seedDb();
    addMsg(db, { source: 'slack', channel: '#team', role: 'silas', author: 'silas', content: 'a', timestamp: ONE_HOUR_AGO });
    addMsg(db, { source: 'claude', channel: 'session:silas', role: 'silas', author: 'assistant', content: 'b', timestamp: ONE_HOUR_AGO });
    const body = fetchChorusReconcile(deps(db), { role: 'wren' }).body as { stats: { total: number; bySource: Record<string, number> } };
    expect(body.stats.total).toBe(2);
    expect(body.stats.bySource).toEqual({ slack: 1, claude: 1 });
    db.close();
  });
});
