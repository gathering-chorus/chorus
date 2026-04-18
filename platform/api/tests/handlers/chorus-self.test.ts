/**
 * chorus-self handler — unit tests (#2189).
 *
 * In-memory SQLite with FTS5 contentless-ish setup seeded across all sources.
 * Tests verify whitelist enforcement, graceful degradation when semantic/sparql
 * unavailable, limit clamping, and event emission.
 */
import Database from 'better-sqlite3';
import { fetchSelf, type SelfDeps } from '../../src/handlers/chorus-self';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT, channel TEXT, role TEXT, content TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');
    CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END;
  `);
  const ins = db.prepare('INSERT INTO messages (source, channel, role, content, timestamp) VALUES (?, ?, ?, ?, ?)');
  ins.run('memory', 'mem', 'kade', 'apple orchard notes', '2026-04-18T10:00:00Z');
  ins.run('story', 'st', 'wren', 'apple story draft', '2026-04-18T09:00:00Z');
  ins.run('brief', 'br', 'silas', 'apple brief from silas', '2026-04-18T08:00:00Z');
  ins.run('claude', 'se', 'kade', 'apple session raw', '2026-04-18T07:00:00Z');
  ins.run('spine', 'sp', 'kade', 'apple spine event', '2026-04-18T06:00:00Z');
  return db;
}

function passthroughMerge(fts: unknown[], sem: unknown[], sparql: unknown[]): unknown[] {
  return [...fts, ...sem, ...sparql];
}

function deps(db: Database.Database, overrides: Partial<SelfDeps> = {}): SelfDeps {
  return {
    db,
    mergeUnified: passthroughMerge,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchSelf (#2189 /api/chorus/self)', () => {
  test('missing q → 400', async () => {
    const db = seedDb();
    const r = await fetchSelf(deps(db), {});
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'Missing required parameter: q' });
    db.close();
  });

  test('FTS excludes non-whitelist sources (claude, spine)', async () => {
    const db = seedDb();
    const r = await fetchSelf(deps(db), { q: 'apple' });
    const body = r.body as { results: Array<{ source: string }>; sources: { fts: number } };
    expect(body.sources.fts).toBe(3);
    const sources = body.results.map((x) => x.source);
    expect(sources).not.toContain('claude');
    expect(sources).not.toContain('spine');
    db.close();
  });

  test('semantic results filtered to whitelist', async () => {
    const db = seedDb();
    const r = await fetchSelf(
      deps(db, {
        semanticSearch: async () => [
          { msg_id: 1, source: 'memory', content: 'a' },
          { msg_id: 2, source: 'claude', content: 'b' },
          { msg_id: 3, source: 'brief', content: 'c' },
        ],
      }),
      { q: 'apple' },
    );
    const body = r.body as { sources: { semantic: number } };
    expect(body.sources.semantic).toBe(2);
    db.close();
  });

  test('semantic absent → fts + sparql still work', async () => {
    const db = seedDb();
    const r = await fetchSelf(
      deps(db, { sparqlSearch: async () => [{ uri: 'urn:x' }] }),
      { q: 'apple' },
    );
    const body = r.body as { sources: { semantic: number; sparql: number } };
    expect(body.sources.semantic).toBe(0);
    expect(body.sources.sparql).toBe(1);
    db.close();
  });

  test('sparqlSearch throws → swallowed', async () => {
    const db = seedDb();
    const r = await fetchSelf(
      deps(db, { sparqlSearch: async () => { throw new Error('fuseki down'); } }),
      { q: 'apple' },
    );
    expect(r.status).toBe(200);
    const body = r.body as { sources: { sparql: number; fts: number } };
    expect(body.sources.sparql).toBe(0);
    expect(body.sources.fts).toBe(3);
    db.close();
  });

  test('semanticSearch throws → swallowed', async () => {
    const db = seedDb();
    const r = await fetchSelf(
      deps(db, { semanticSearch: async () => { throw new Error('lance down'); } }),
      { q: 'apple' },
    );
    expect(r.status).toBe(200);
    const body = r.body as { sources: { semantic: number } };
    expect(body.sources.semantic).toBe(0);
    db.close();
  });

  test('limit capped at 50 (semantic over-fetch = 150)', async () => {
    const db = seedDb();
    let captured = 0;
    await fetchSelf(
      deps(db, {
        semanticSearch: async (_q, l) => { captured = l; return []; },
      }),
      { q: 'apple', limit: '9999' },
    );
    expect(captured).toBe(150);
    db.close();
  });

  test('default limit = 10', async () => {
    const db = seedDb();
    let captured = 0;
    await fetchSelf(
      deps(db, {
        semanticSearch: async (_q, l) => { captured = l; return []; },
      }),
      { q: 'apple' },
    );
    expect(captured).toBe(30);
    db.close();
  });

  test('emitSearchEvent called with counts + duration', async () => {
    const db = seedDb();
    const events: Record<string, unknown>[] = [];
    let t = 1000;
    await fetchSelf(
      deps(db, {
        emitSearchEvent: (f) => events.push(f),
        now: () => (t += 5),
      }),
      { q: 'apple' },
    );
    expect(events).toHaveLength(1);
    expect(events[0].system).toBe('chorus-self');
    expect(events[0].mode).toBe('self');
    expect(events[0].duration_ms).toBe(5);
    expect(String(events[0].sources)).toContain('fts=3');
    db.close();
  });

  test('filter.whitelist echoed in response', async () => {
    const db = seedDb();
    const r = await fetchSelf(deps(db), { q: 'apple' });
    const body = r.body as { filter: { whitelist: string[] } };
    expect(body.filter.whitelist.sort()).toEqual(
      ['adr', 'brief', 'decision', 'memory', 'story'].sort(),
    );
    db.close();
  });
});
