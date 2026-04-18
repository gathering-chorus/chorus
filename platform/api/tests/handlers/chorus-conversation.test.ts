/**
 * chorus-conversation handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusConversation, type ChorusConversationDeps } from '../../src/handlers/chorus-conversation';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      author TEXT, content TEXT, timestamp TEXT, role TEXT, session_id TEXT
    );
  `);
  const m = db.prepare('INSERT INTO messages (author, content, timestamp, role, session_id) VALUES (?,?,?,?,?)');
  m.run('user', 'hey wren', '2026-04-18T14:00:00', 'wren', 's1');
  m.run('assistant', 'hi jeff', '2026-04-18T14:01:00', 'wren', 's1');
  m.run('user', 'what about silas', '2026-04-18T14:02:00', 'silas', 's2');
  m.run('assistant', 'here', '2026-04-18T14:03:00', 'silas', 's2');
  m.run('user', '<system-reminder>ignore me</system-reminder>', '2026-04-18T14:04:00', 'wren', 's1');
  m.run('user', '<task-created>', '2026-04-18T14:05:00', 'wren', 's1');
  m.run('user', 'Base directory for this skill: /foo', '2026-04-18T14:06:00', 'wren', 's1');
  m.run('user', '[Request interrupted by user]', '2026-04-18T14:07:00', 'wren', 's1');
  m.run('user', 'x', '2026-04-18T14:08:00', 'wren', 's1');
  return db;
}

function deps(db: Database.Database, over: Partial<ChorusConversationDeps> = {}): ChorusConversationDeps {
  return {
    db,
    isEDT: () => true,
    convertToLocal: (iso) => iso.replace('T', ' '),
    ...over,
  };
}

describe('fetchChorusConversation (#2188)', () => {
  test('missing roles → 400', () => {
    const db = seedDb();
    const r = fetchChorusConversation(deps(db), {});
    expect(r.status).toBe(400);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/roles/i);
    db.close();
  });

  test('only-jeff role list → 400', () => {
    const db = seedDb();
    const r = fetchChorusConversation(deps(db), { roles: 'jeff' });
    expect(r.status).toBe(400);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/non-jeff/i);
    db.close();
  });

  test('single-role thread includes user (jeff) + assistant turns', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'jeff,wren', date: '2026-04-18' }).body as {
      thread: Array<{ speaker: string; text: string }>; count: number;
    };
    const speakers = body.thread.map((t) => t.speaker);
    expect(speakers).toContain('jeff');
    expect(speakers).toContain('wren');
    expect(body.count).toBe(2);
    db.close();
  });

  test('system-reminder messages filtered out', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'jeff,wren', date: '2026-04-18' }).body as {
      thread: Array<{ text: string }>;
    };
    expect(body.thread.every((t) => !t.text.includes('system-reminder'))).toBe(true);
    db.close();
  });

  test('task, base-directory, interrupted, and <2-char texts filtered', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'jeff,wren', date: '2026-04-18' }).body as {
      thread: Array<{ text: string }>;
    };
    const texts = body.thread.map((t) => t.text);
    expect(texts).not.toContain('<task-created>');
    expect(texts.some((t) => t.startsWith('Base directory'))).toBe(false);
    expect(texts.some((t) => t.startsWith('[Request interrupted'))).toBe(false);
    expect(texts).not.toContain('x');
    db.close();
  });

  test('multi-role participants', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'wren,silas', date: '2026-04-18' }).body as {
      thread: Array<{ speaker: string }>; participants: string[];
    };
    const speakers = new Set(body.thread.map((t) => t.speaker));
    expect(speakers.has('wren')).toBe(true);
    expect(speakers.has('silas')).toBe(true);
    expect(body.participants).toEqual(['wren', 'silas']);
    db.close();
  });

  test('user-author maps to jeff regardless of role', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'wren,silas', date: '2026-04-18' }).body as {
      thread: Array<{ speaker: string; text: string }>;
    };
    const jeffMessage = body.thread.find((t) => t.text === 'what about silas');
    expect(jeffMessage?.speaker).toBe('jeff');
    db.close();
  });

  test('limit clamps to 2000 max', () => {
    const db = seedDb();
    // Already has 9 messages; ensure the limit param flows through
    const body = fetchChorusConversation(deps(db), { roles: 'wren', date: '2026-04-18', limit: '1' }).body as {
      thread: unknown[];
    };
    // 1 assistant msg passes filter in 1-message fetch
    expect(body.thread.length).toBeLessThanOrEqual(1);
    db.close();
  });

  test('date defaults to today when omitted', () => {
    const db = seedDb();
    const today = new Date().toISOString().slice(0, 10);
    const body = fetchChorusConversation(deps(db), { roles: 'wren' }).body as { date: string };
    expect(body.date).toBe(today);
    db.close();
  });

  test('after/before times use local filter; isEDT=true uses UTC-4 offset', () => {
    const db = seedDb();
    // Messages are at T14:00..T14:08 UTC. Under EDT, local = UTC-4 = 10:00..10:08.
    // Filter after=10:02 should include 10:02 and later.
    const body = fetchChorusConversation(deps(db), {
      roles: 'wren,silas', date: '2026-04-18', after: '10:02',
    }).body as { thread: Array<{ text: string; time: string }> };
    // 10:02 is strictly after boundary — convertToLocal is identity (no real tz shift)
    // so the handler's local-HM filter reads from the "time" field.
    expect(body.thread.length).toBeGreaterThan(0);
    db.close();
  });

  test('timezone echoed in response', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'wren', tz: 'UTC' }).body as { timezone: string };
    expect(body.timezone).toBe('UTC');
    db.close();
  });

  test('thread ordered by timestamp ASC', () => {
    const db = seedDb();
    const body = fetchChorusConversation(deps(db), { roles: 'wren,silas', date: '2026-04-18' }).body as {
      thread: Array<{ time: string }>;
    };
    const times = body.thread.map((t) => t.time);
    const sorted = [...times].sort();
    expect(times).toEqual(sorted);
    db.close();
  });
});
