// @test-type: unit — injected fake verifier + role lookup + in-memory SQLite; no live world (#3528).
// @domain: search — placed by chorus-crawl's domain rules (#4538)
/**
 * #4779 — Verified session turn push + channel-scoped boot-read isolation.
 *
 * Silas requirement (2026-10-28):
 *   1. Each session pushes its own turns; the role comes from its verified
 *      CSS identity, never from folder names or cross-home indexing.
 *   2. Negative proof (#3734): Kade's turns never appear in Silas's boot-read,
 *      and unauthenticated / cross-lane pushes are refused (401 / 403).
 */
import Database from 'better-sqlite3';
import { fetchSearch, type SearchDeps } from '../../src/handlers/chorus-search';
import { handleSessionTurnPush, type SessionTurnDeps } from '../../src/handlers/chorus-session-turn';

function createDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      source TEXT NOT NULL,
      source_id TEXT UNIQUE,
      channel TEXT,
      role TEXT,
      author TEXT,
      content TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      session_id TEXT,
      thread_id TEXT,
      is_bridge INTEGER DEFAULT 0,
      metadata TEXT
    );
    CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');
    CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END;
    CREATE TABLE watermarks (source TEXT PRIMARY KEY, last_seen TEXT, last_indexed TEXT);
  `);
  return db;
}

function searchDeps(db: Database.Database): SearchDeps {
  return {
    db,
    sparqlSearch: async () => [],
    mergeUnified: (a, b, c = []) => [...a, ...b, ...c],
    mergeRRF: (a, b) => [...a, ...b],
    buildSearchMeta: () => ({ schema_version: '1.0.0' }),
    enrichHit: (r) => r,
    resolveSearchLimit: (raw) => {
      if (!raw) return { limit: 5, explicit: false };
      const n = parseInt(raw, 10) || 5;
      return { limit: Math.min(n, 100), explicit: true };
    },
    now: () => 1_000_000,
  };
}

const okVerify = (webId: string) => async (_cred: string) => ({
  ok: true as const,
  webId,
  scope: ['session-write'],
});
const badVerify = async (_cred: string) => ({
  ok: false as const,
  reason: 'invalid-signature',
});
const roleOf = (map: Record<string, string>) => async (webId: string) => map[webId] ?? null;

const SILAS_WEBID = 'https://id.lightlifeurbangardens.com/silas/profile/card#me';
const KADE_WEBID = 'https://id.lightlifeurbangardens.com/kade/profile/card#me';
const ROLE_MAP: Record<string, string> = {
  [SILAS_WEBID]: 'silas',
  [KADE_WEBID]: 'kade',
};

describe('#4779 handleSessionTurnPush', () => {
  test('negative proof (#3734): missing or invalid credential → 401 authn-missing and zero rows written', async () => {
    const db = createDb();
    const deps: SessionTurnDeps = { db, verify: badVerify, roleForWebId: roleOf(ROLE_MAP) };

    const r1 = await handleSessionTurnPush(
      {
        authorization: '',
        body: { turns: [{ author: 'assistant', content: 'unauthenticated turn' }] },
      },
      deps,
    );
    expect(r1.status).toBe(401);
    expect(r1.body).toMatchObject({ error: 'authn-missing' });

    const r2 = await handleSessionTurnPush(
      {
        authorization: 'Bearer forged',
        body: { turns: [{ author: 'assistant', content: 'forged turn' }] },
      },
      deps,
    );
    expect(r2.status).toBe(401);
    expect(r2.body).toMatchObject({ error: 'authn-missing' });

    const count = (db.prepare('SELECT COUNT(*) as n FROM messages').get() as { n: number }).n;
    expect(count).toBe(0);
    db.close();
  });

  test('negative proof (#3734): unknown WebID with no held role → 403 no-role-held', async () => {
    const db = createDb();
    const deps: SessionTurnDeps = {
      db,
      verify: okVerify('https://id.lightlifeurbangardens.com/outsider/profile/card#me'),
      roleForWebId: roleOf(ROLE_MAP),
    };
    const r = await handleSessionTurnPush(
      {
        authorization: 'Bearer x',
        body: { turns: [{ author: 'assistant', content: 'outsider turn' }] },
      },
      deps,
    );
    expect(r.status).toBe(403);
    expect(r.body).toMatchObject({ error: 'no-role-held' });
    db.close();
  });

  test('negative proof (#3734): verified kade credential claiming silas lane → 403 not-your-lane', async () => {
    const db = createDb();
    const deps: SessionTurnDeps = {
      db,
      verify: okVerify(KADE_WEBID),
      roleForWebId: roleOf(ROLE_MAP),
    };
    const r = await handleSessionTurnPush(
      {
        authorization: 'Bearer x',
        body: {
          role: 'silas',
          turns: [{ author: 'assistant', content: 'spoofed silas turn from kade' }],
        },
      },
      deps,
    );
    expect(r.status).toBe(403);
    expect(r.body).toMatchObject({ error: 'not-your-lane' });
    const count = (db.prepare('SELECT COUNT(*) as n FROM messages').get() as { n: number }).n;
    expect(count).toBe(0);
    db.close();
  });

  test('verified silas credential writes user+assistant turns under channel=session:silas idempotently', async () => {
    const db = createDb();
    const deps: SessionTurnDeps = {
      db,
      verify: okVerify(SILAS_WEBID),
      roleForWebId: roleOf(ROLE_MAP),
    };
    const r = await handleSessionTurnPush(
      {
        authorization: 'Bearer x',
        body: {
          session_id: 'silas-sess-1',
          source: 'antigravity',
          turns: [
            {
              author: 'user',
              content: 'check the identity graph',
              timestamp: '2026-10-28T16:00:00Z',
              source_id: 'agy:silas-sess-1:1',
            },
            {
              author: 'assistant',
              content: 'All 5 agent principals verified in Fuseki.',
              timestamp: '2026-10-28T16:00:05Z',
              source_id: 'agy:silas-sess-1:2',
            },
          ],
        },
      },
      deps,
    );
    expect(r.status).toBe(200);
    expect(r.body).toMatchObject({ status: 'ok', role: 'silas', channel: 'session:silas', inserted: 2 });

    // Re-pushing identical source_id is idempotent (inserted: 0)
    const rDup = await handleSessionTurnPush(
      {
        authorization: 'Bearer x',
        body: {
          session_id: 'silas-sess-1',
          source: 'antigravity',
          turns: [
            {
              author: 'assistant',
              content: 'All 5 agent principals verified in Fuseki.',
              timestamp: '2026-10-28T16:00:05Z',
              source_id: 'agy:silas-sess-1:2',
            },
          ],
        },
      },
      deps,
    );
    expect(rDup.status).toBe(200);
    expect(rDup.body).toMatchObject({ inserted: 0 });

    const rows = db
      .prepare('SELECT source, channel, role, author, content FROM messages ORDER BY timestamp ASC')
      .all() as Array<{ source: string; channel: string; role: string; author: string; content: string }>;
    expect(rows).toEqual([
      {
        source: 'antigravity',
        channel: 'session:silas',
        role: 'jeff',
        author: 'user',
        content: 'check the identity graph',
      },
      {
        source: 'antigravity',
        channel: 'session:silas',
        role: 'silas',
        author: 'assistant',
        content: 'All 5 agent principals verified in Fuseki.',
      },
    ]);
    db.close();
  });

  test('negative proof (#3734): 30 kade turns pushed via kade identity never appear in silas boot-read search', async () => {
    const db = createDb();
    const silasDeps: SessionTurnDeps = {
      db,
      verify: okVerify(SILAS_WEBID),
      roleForWebId: roleOf(ROLE_MAP),
    };
    const kadeDeps: SessionTurnDeps = {
      db,
      verify: okVerify(KADE_WEBID),
      roleForWebId: roleOf(ROLE_MAP),
    };

    await handleSessionTurnPush(
      {
        authorization: 'Bearer x',
        body: {
          session_id: 'silas-s1',
          turns: [
            {
              author: 'user',
              content: 'silas boot question from yesterday',
              timestamp: '2026-10-27T18:00:00Z',
            },
            {
              author: 'assistant',
              content: 'silas boot answer from yesterday',
              timestamp: '2026-10-27T18:01:00Z',
            },
          ],
        },
      },
      silasDeps,
    );

    const kadeTurns = Array.from({ length: 30 }, (_, i) => ({
      author: (i % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: `the kade turn number ${i}`,
      timestamp: `2026-10-28T16:${String(i).padStart(2, '0')}:00Z`,
    }));
    await handleSessionTurnPush(
      {
        authorization: 'Bearer y',
        body: { session_id: 'kade-s1', turns: kadeTurns },
      },
      kadeDeps,
    );

    const bootSearch = await fetchSearch(searchDeps(db), {
      channel: 'session:silas',
      order: 'recent',
      limit: '30',
    });
    expect(bootSearch.status).toBe(200);
    const results = (bootSearch.body as { results: Array<{ channel: string; role: string; content: string }> }).results;
    expect(results).toHaveLength(2);
    expect(results.every((r) => r.channel === 'session:silas')).toBe(true);
    expect(results.some((r) => r.channel === 'session:kade' || r.role === 'kade')).toBe(false);
    expect(results.map((r) => r.content)).toEqual([
      'silas boot answer from yesterday',
      'silas boot question from yesterday',
    ]);
    db.close();
  });
});
