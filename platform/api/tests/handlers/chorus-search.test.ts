/**
 * chorus-search handler — unit tests (#2189).
 *
 * In-memory SQLite with FTS5, stubs for semantic/sparql/merge/meta fns.
 * Covers each mode + fallback paths + limit + role filter + truncation.
 */
import Database from 'better-sqlite3';
import { fetchSearch, type SearchDeps } from '../../src/handlers/chorus-search';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT, channel TEXT, role TEXT, author TEXT, content TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');
    CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END;
    CREATE TABLE watermarks (source TEXT PRIMARY KEY, last_indexed TEXT);
  `);
  const ins = db.prepare(
    'INSERT INTO messages (source, channel, role, author, content, timestamp) VALUES (?, ?, ?, ?, ?, ?)',
  );
  ins.run('slack', 'eng', 'kade', 'kade', 'apple pie discussion', '2026-04-18T10:00:00Z');
  ins.run('slack', 'eng', 'wren', 'wren', 'apple deployment', '2026-04-18T09:00:00Z');
  ins.run('memory', 'mem', 'kade', 'kade', 'apple retrospective', '2026-04-18T08:00:00Z');
  ins.run('claude', 'se', 'silas', 'silas', 'apple session log', '2026-04-18T07:00:00Z');
  return db;
}

function passthroughMerge(a: unknown[], b: unknown[], c: unknown[] = []): unknown[] {
  return [...a, ...b, ...c];
}

function deps(db: Database.Database, overrides: Partial<SearchDeps> = {}): SearchDeps {
  return {
    db,
    sparqlSearch: async () => [],
    mergeUnified: passthroughMerge,
    mergeRRF: (a, b) => [...a, ...b],
    buildSearchMeta: () => ({ schema_version: '1.0.0' }),
    enrichHit: (r, _n) => ({ ...(r as object), enriched: true }),
    resolveSearchLimit: (raw) => {
      if (!raw) return { limit: 5, explicit: false };
      const n = parseInt(raw, 10) || 5;
      return { limit: Math.min(n, 100), explicit: true };
    },
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchSearch (#2189 /api/chorus/search)', () => {
  test('missing q → 400', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), {});
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'Missing required parameter: q' });
    db.close();
  });

  test('default mode=fts returns results with timestamp DESC', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple' });
    expect(r.status).toBe(200);
    const b = r.body as { results: Array<{ content: string }>; mode: string };
    expect(b.mode).toBe('fts');
    expect(b.results.length).toBeGreaterThan(0);
    // Most recent first
    expect(b.results[0].content).toBe('apple pie discussion');
    db.close();
  });

  test('role filter limits FTS results', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple', role: 'kade' });
    const b = r.body as { results: Array<{ role: string }> };
    expect(b.results.every((x) => x.role === 'kade')).toBe(true);
    db.close();
  });

  test('mode=relevance uses bm25 ordering', async () => {
    const db = seedDb();
    // smoke only — ensure 200 and mode echoed
    const r = await fetchSearch(deps(db), { q: 'apple', mode: 'relevance' });
    expect(r.status).toBe(200);
    const b = r.body as { mode: string };
    expect(b.mode).toBe('relevance');
    db.close();
  });

  test('mode=semantic without semanticSearch dep → error envelope', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple', mode: 'semantic' });
    expect(r.status).toBe(200);
    const b = r.body as { error: string; mode: string };
    expect(b.error).toBe('Semantic index not available');
    expect(b.mode).toBe('semantic');
    db.close();
  });

  test('mode=semantic with fn returns results + emits event', async () => {
    const db = seedDb();
    const events: Record<string, unknown>[] = [];
    const r = await fetchSearch(
      deps(db, {
        semanticSearch: async () => [{ msg_id: 99, source: 'memory', content: 'semantic hit' }],
        emitSearchEvent: (f) => events.push(f),
      }),
      { q: 'apple', mode: 'semantic' },
    );
    const b = r.body as { mode: string; results: unknown[] };
    expect(b.mode).toBe('semantic');
    expect(b.results).toHaveLength(1);
    expect(events[0].mode).toBe('semantic');
    db.close();
  });

  test('mode=semantic when semanticSearch throws → 500', async () => {
    const db = seedDb();
    const r = await fetchSearch(
      deps(db, { semanticSearch: async () => { throw new Error('lance crash'); } }),
      { q: 'apple', mode: 'semantic' },
    );
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toContain('lance crash');
    db.close();
  });

  test('mode=unified merges fts+semantic+sparql with source counts', async () => {
    const db = seedDb();
    const r = await fetchSearch(
      deps(db, {
        semanticSearch: async () => [{ msg_id: 100, source: 'memory' }],
        sparqlSearch: async () => [{ uri: 'urn:x' }],
      }),
      { q: 'apple', mode: 'unified' },
    );
    const b = r.body as { mode: string; sources: { fts: number; semantic: number; sparql: number } };
    expect(b.mode).toBe('unified');
    expect(b.sources.semantic).toBe(1);
    expect(b.sources.sparql).toBe(1);
    expect(b.sources.fts).toBeGreaterThan(0);
    db.close();
  });

  test('mode=hybrid merges fts+semantic', async () => {
    const db = seedDb();
    const r = await fetchSearch(
      deps(db, {
        semanticSearch: async () => [{ msg_id: 200, source: 'memory', content: 'sem' }],
      }),
      { q: 'apple', mode: 'hybrid' },
    );
    const b = r.body as { mode: string };
    expect(b.mode).toBe('hybrid');
    db.close();
  });

  test('mode=hybrid without semanticSearch falls through to fts', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple', mode: 'hybrid' });
    const b = r.body as { mode: string };
    expect(b.mode).toBe('fts');
    db.close();
  });

  test('default limit = 5 via resolveSearchLimit', async () => {
    const db = seedDb();
    // seed 10 matching rows
    const ins = db.prepare('INSERT INTO messages (source, channel, role, content, timestamp) VALUES (?, ?, ?, ?, ?)');
    for (let i = 0; i < 10; i++) ins.run('slack', 'c', 'kade', `apple-${i}`, `2026-04-18T${String(10 + i).padStart(2, '0')}:00:00Z`);
    const r = await fetchSearch(deps(db), { q: 'apple' });
    const b = r.body as { results: unknown[]; _meta: { limit_applied: number; truncated: boolean } };
    expect(b._meta.limit_applied).toBe(5);
    expect(b.results).toHaveLength(5);
    expect(b._meta.truncated).toBe(true);
    db.close();
  });

  test('explicit limit param overrides default', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple', limit: '3' });
    const b = r.body as { _meta: { limit_applied: number; limit_default: boolean } };
    expect(b._meta.limit_applied).toBe(3);
    expect(b._meta.limit_default).toBe(false);
    db.close();
  });

  test('emitSearchEvent called with mode+duration+role_filter', async () => {
    const db = seedDb();
    const events: Record<string, unknown>[] = [];
    let t = 1000;
    await fetchSearch(
      deps(db, { emitSearchEvent: (f) => events.push(f), now: () => (t += 5) }),
      { q: 'apple', role: 'kade' },
    );
    expect(events).toHaveLength(1);
    expect(events[0].mode).toBe('fts');
    expect(events[0].role_filter).toBe('kade');
    expect(events[0].duration_ms).toBe(5);
    db.close();
  });

  test('unified mode: sparql throws → falls through to fts', async () => {
    const db = seedDb();
    const r = await fetchSearch(
      deps(db, { sparqlSearch: async () => { throw new Error('fuseki down'); } }),
      { q: 'apple', mode: 'unified' },
    );
    const b = r.body as { mode: string };
    expect(b.mode).toBe('fts');
    db.close();
  });

  test('enrichHit runs on every returned result', async () => {
    const db = seedDb();
    const r = await fetchSearch(deps(db), { q: 'apple' });
    const b = r.body as { results: Array<{ enriched: boolean }> };
    expect(b.results.every((x) => x.enriched === true)).toBe(true);
    db.close();
  });
});
