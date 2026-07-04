// @test-type: unit — in-memory sqlite + fake fetch/exec deps; no Fuseki, no Loki, no live services.
/**
 * chorus-crawl handler — unit tests (#2189).
 *
 * Tests the contract boundaries: 404 gate, everything-fails-silently fallback,
 * card aggregation, spine event filtering by card id, trust score math,
 * related domain counting, known-domain lookup, hardcoded endpoint registry.
 */
import Database from 'better-sqlite3';
import {
  fetchCrawl,
  type CrawlDeps,
  type BoardCard,
  type FetchFn,
  type ExecAsyncFn,
  type AthenaSparqlFn,
} from '../../src/handlers/chorus-crawl';

function emptyDb(): Database.Database {
  const db = new Database(':memory:');
  // #3054: mirror the real index — external-content FTS5 over messages, kept in
  // sync by an insert trigger — so collectMentions' MATCH path is exercised here.
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, author TEXT, content TEXT, role TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');
    CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END;
  `);
  return db;
}

const nullFetch: FetchFn = async () => ({ ok: false, status: 503, json: async () => ({}) });
const nullExec: ExecAsyncFn = async () => ({ stdout: '' });
const nullSparql: AthenaSparqlFn = async () => ({ results: { bindings: [] } });

// #3088: spine now comes from Loki, not a file tail. Returns card.* lines for the
// spine query (routed by the {filename=...} label); not-ok for other fetch calls.
function lokiSpineFetch(cardLines: string[]): FetchFn {
  return (async (url: string) => {
    if (url.includes('filename')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ data: { result: [{ values: cardLines.map((l, i) => [String(i * 1000), l]) }] } }),
      };
    }
    return { ok: false, status: 503, json: async () => ({}) };
  }) as unknown as FetchFn;
}

function deps(overrides: Partial<CrawlDeps> = {}): CrawlDeps {
  return {
    db: emptyDb(),
    getBoardCards: () => [],
    fetchFn: nullFetch,
    athenaSparqlQuery: nullSparql,
    execAsync: nullExec,
    readFile: () => '',
    exists: () => false,
    readdir: () => [],
    now: () => 1_700_000_000_000,
    chorusLogPath: '/fake/chorus.log',
    memoryDir: '/fake/memory',
    alertDir: '/fake/alerting',
    ...overrides,
  };
}

describe('fetchCrawl (#2189 /api/chorus/crawl/:domain)', () => {
  test('unknown domain → 404 with suggestion + valid_count', async () => {
    const r = await fetchCrawl('notarealdomain', deps());
    expect(r.status).toBe(404);
    const b = r.body as { error: string; suggestion: string; valid_count: number };
    expect(b.error).toContain("'notarealdomain' not found");
    expect(b.suggestion).toContain('photos');
    expect(b.valid_count).toBeGreaterThan(20);
  });

  test('domain case-insensitive', async () => {
    const r = await fetchCrawl('Photos', deps());
    expect(r.status).toBe(200);
    expect((r.body as { domain: string }).domain).toBe('photos');
  });

  test('empty sources → 200 with all-empty aggregates + trust_score 50', async () => {
    const r = await fetchCrawl('photos', deps());
    const b = r.body as { cards: unknown[]; related: unknown[]; history: { trust_score: number; health: string } };
    expect(b.cards).toEqual([]);
    expect(b.related).toEqual([]);
    expect(b.history.trust_score).toBe(50);
    expect(b.history.health).toBe('attention');
  });

  test('cards with matching domain tag included; unresolved filter', async () => {
    const boardCards: BoardCard[] = [
      { id: '1', title: 'Photos card A', status: 'Next', owner: 'kade', tags: 'domain:photos' },
      { id: '2', title: 'Photos card B', status: 'Done', owner: 'kade', tags: 'domain:photos' },
      { id: '3', title: 'Music card', status: 'Next', owner: 'wren', tags: 'domain:music' }, // excluded
    ];
    const r = await fetchCrawl('photos', deps({ getBoardCards: () => boardCards }));
    const b = r.body as { cards: Array<{ index: number }>; history: { unresolved: unknown[] } };
    expect(b.cards).toHaveLength(2);
    expect(b.history.unresolved).toHaveLength(1);
  });

  test('trust_score rewards Done cards, penalises unresolved + feedback', async () => {
    const boardCards: BoardCard[] = [
      { id: '1', title: 'x', status: 'Done', owner: 'kade', tags: 'domain:photos' },
      { id: '2', title: 'y', status: 'Done', owner: 'kade', tags: 'domain:photos' },
      { id: '3', title: 'z', status: 'Next', owner: 'kade', tags: 'domain:photos' },
    ];
    const r = await fetchCrawl('photos', deps({ getBoardCards: () => boardCards }));
    const b = r.body as { history: { trust_score: number; health: string } };
    // 50 baseline + activityBonus 20 (2*10) - unresolved 10 = 60
    expect(b.history.trust_score).toBe(60);
    expect(b.history.health).toBe('healthy');
  });

  test('session mentions filtered: system-reminders and short text dropped', async () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO messages (author, content, role, timestamp) VALUES (?, ?, ?, ?)');
    ins.run('kade', 'Real mention about photos domain and the harvesting pipeline work', 'kade', '2026-04-18T10:00:00Z');
    ins.run('kade', '<system-reminder>photos reminder</system-reminder>', 'kade', '2026-04-18T09:00:00Z');
    ins.run('kade', 'too short photos', 'kade', '2026-04-18T08:00:00Z');
    const r = await fetchCrawl('photos', deps({ db }));
    const b = r.body as { mentions: unknown[] };
    expect(b.mentions).toHaveLength(1);
    db.close();
  });

  test('related-domain counts derived from cross-domain text', async () => {
    const db = emptyDb();
    db.prepare('INSERT INTO messages (author, content, role, timestamp) VALUES (?, ?, ?, ?)').run(
      'kade',
      'The photos pipeline depends on the music harvesting flow and notes indexing system',
      'kade',
      '2026-04-18T10:00:00Z',
    );
    const r = await fetchCrawl('photos', deps({ db }));
    const b = r.body as { related: Array<{ domain: string; strength: number }> };
    const names = b.related.map((x) => x.domain);
    expect(names).toContain('music');
    expect(names).toContain('notes');
    db.close();
  });

  test('spine events filtered to known card ids', async () => {
    const boardCards: BoardCard[] = [
      { id: '42', title: 'x', status: 'Done', owner: 'kade', tags: 'domain:photos' },
    ];
    const logLines = [
      JSON.stringify({ event: 'card.accepted', card: '42', role: 'wren', timestamp: '2026-04-18T10:00:00Z' }),
      JSON.stringify({ event: 'card.moved', card: '999', role: 'wren', timestamp: '2026-04-18T09:00:00Z' }), // unknown card
      JSON.stringify({ event: 'system.other', card: '42', role: 'wren', timestamp: '2026-04-18T08:00:00Z' }), // non-card event
      'not json',
    ];
    const r = await fetchCrawl('photos', deps({
      getBoardCards: () => boardCards,
      fetchFn: lokiSpineFetch(logLines),
    }));
    const b = r.body as { spine: Array<{ event: string }> };
    expect(b.spine).toHaveLength(1);
    expect(b.spine[0].event).toBe('card.accepted');
  });

  test('#3088: spine query is TIME-BOUNDED (Loki range, not unbounded)', async () => {
    let spineUrl = '';
    const capturingFetch = (async (url: string) => {
      if (url.includes('filename')) spineUrl = url;
      return { ok: false, status: 503, json: async () => ({}) };
    }) as unknown as FetchFn;
    await fetchCrawl('photos', deps({
      getBoardCards: () => [{ id: '42', title: 'x', status: 'Done', owner: 'kade', tags: 'domain:photos' }],
      fetchFn: capturingFetch,
    }));
    // collectSpine must query a bounded time range, never an unbounded scan.
    expect(spineUrl.includes('query_range')).toBe(true);
    expect(spineUrl).toContain('start=');
    expect(spineUrl).toContain('end=');
  });

  test('memory feedback: only files containing domain in body counted', async () => {
    const r = await fetchCrawl('photos', deps({
      exists: (p) => p === '/fake/memory',
      readdir: () => ['feedback_photos.md', 'feedback_music.md', 'other_file.md'],
      readFile: (p) => {
        if (p.endsWith('feedback_photos.md')) return 'name: Photos feedback\nbody contains photos';
        if (p.endsWith('feedback_music.md')) return 'name: Music feedback\nbody about music only';
        return '';
      },
    }));
    const b = r.body as { history: { feedback: string[] } };
    expect(b.history.feedback).toEqual(['Photos feedback']);
  });

  test('infra: seeds domain → hardcoded endpoints + monitoring', async () => {
    const r = await fetchCrawl('seeds', deps());
    const b = r.body as { infra: { endpoints: string[]; monitoring: string[] } };
    expect(b.infra.endpoints.some((e) => e.includes('/api/chorus/seeds'))).toBe(true);
    expect(b.infra.monitoring).toContain('seed-probe LaunchAgent');
  });

  test('infra: chorus domain → different endpoint set', async () => {
    const r = await fetchCrawl('chorus', deps());
    const b = r.body as { infra: { endpoints: string[] } };
    expect(b.infra.endpoints.some((e) => e.includes('Socket.IO'))).toBe(true);
  });

  // #2620 AC1: server.ts wires alertDir at the canonical alert-rule home in
  // shared-observability. Before the fix, alertDir resolved to chorus/alerting/
  // which doesn't exist, so fetchCrawl returned alerts: [] for every domain
  // — the cucumber @crawler @alerts scenario could never pass.
  // This test runs fetchCrawl with the same path server.ts builds and the
  // real fs deps, then asserts the chorus-alerts.yaml rules surface.
  test('#2620: fetchCrawl returns alerts when alertDir is wired to canonical path', async () => {
    const fs = require('fs');
    // Try-both candidates mirroring server.ts (CHORUS_ROOT is /chorus in prod
    // launchagent and the parent in dev). Pick whichever exists; skip if
    // neither (CI without shared-observability checked out).
    const candidates = [
      '/Users/jeffbridwell/CascadeProjects/shared-observability/config/grafana/provisioning/alerting',
      '/Users/jeffbridwell/CascadeProjects/chorus/../shared-observability/config/grafana/provisioning/alerting',
    ];
    const wired = candidates.find((p) => fs.existsSync(p));
    if (!wired) return;
    const r = await fetchCrawl('chorus', deps({
      exists: (p: string) => fs.existsSync(p),
      readdir: (p: string) => fs.readdirSync(p),
      readFile: (p: string, enc: BufferEncoding) => fs.readFileSync(p, enc),
      alertDir: wired,
    }));
    const b = r.body as { alerts: Array<{ name: string }> };
    expect(b.alerts.length).toBeGreaterThan(0);
  });

  test('alerts: yml files matching domain are collected', async () => {
    const yml = 'alert: Photos pipeline stalled\nseverity: warning\nbody photos domain';
    const r = await fetchCrawl('photos', deps({
      exists: (p) => p === '/fake/alerting',
      readdir: () => ['photos-alerts.yml', 'music-alerts.yml', 'README.txt'],
      readFile: (p) => {
        if (p.endsWith('photos-alerts.yml')) return yml;
        if (p.endsWith('music-alerts.yml')) return 'alert: Music\nseverity: info';
        return '';
      },
    }));
    const b = r.body as { alerts: Array<{ name: string; severity: string }> };
    expect(b.alerts).toHaveLength(1);
    expect(b.alerts[0].name).toBe('Photos pipeline stalled');
    expect(b.alerts[0].severity).toBe('warning');
  });

  test('loki: parses entries, sorts errors first', async () => {
    const r = await fetchCrawl('photos', deps({
      fetchFn: async (url) => {
        if (url.includes('loki/api/v1/query_range')) {
          return {
            ok: true, status: 200,
            json: async () => ({
              data: {
                result: [
                  {
                    values: [
                      ['0', JSON.stringify({ timestamp: '2026-04-18T08:00:00Z', level: 'info', message: 'ok', component: 'a' })],
                      ['0', JSON.stringify({ timestamp: '2026-04-18T09:00:00Z', level: 'error', message: 'bad', component: 'b' })],
                      ['0', 'not json'],
                    ],
                  },
                ],
              },
            }),
          };
        }
        return { ok: false, status: 503, json: async () => ({}) };
      },
    }));
    const b = r.body as { logs: Array<{ level: string }> };
    expect(b.logs).toHaveLength(2);
    expect(b.logs[0].level).toBe('error');
  });

  // #3055: collectMentions must bound the FTS scan (mentionScanCap), so a common
  // domain term can't become a ~400ms synchronous loop block. These lock the cap
  // behaviourally: a revert to ORDER BY (rank|timestamp) over the full match set
  // would ignore the cap, and cap=3 would return more than 3.
  function seedManyMentions(db: Database.Database, n: number): void {
    const ins = db.prepare('INSERT INTO messages (author, content, role, timestamp) VALUES (?,?,?,?)');
    for (let i = 0; i < n; i++) {
      ins.run('jeff', `photos discussion entry number ${i} with sufficient length to pass the filter`, 'jeff', `2026-01-${String((i % 28) + 1).padStart(2, '0')}T00:00:00Z`);
    }
  }

  test('#3055: mentionScanCap bounds the scan — cap=3 yields <=3 mentions despite 20 matches', async () => {
    const db = emptyDb();
    seedManyMentions(db, 20);
    const r = await fetchCrawl('photos', deps({ db, mentionScanCap: 3 }));
    const b = r.body as { mentions: unknown[] };
    expect(b.mentions.length).toBeGreaterThan(0);
    expect(b.mentions.length).toBeLessThanOrEqual(3);
  });

  test('#3055: the cap is load-bearing — a higher cap returns more mentions', async () => {
    const db = emptyDb();
    seedManyMentions(db, 20);
    const r = await fetchCrawl('photos', deps({ db, mentionScanCap: 15 }));
    const b = r.body as { mentions: unknown[] };
    expect(b.mentions.length).toBeGreaterThan(3);
  });

  // #3091 — close the fetch-timeout class: a slow Fuseki (sparqlPost) or slow Loki
  // (collectLogs) used to hang the crawl handler indefinitely (same class #3090
  // closed for collectSpine). Each call now wraps an AbortController + 5s timeout;
  // AbortError → empty bucket for that collector (degrade-to-empty contract).

  test('#3091: SLOW Fuseki (sparqlPost) aborts at 5s — rdf bucket empty, no hang', async () => {
    jest.useFakeTimers();
    // Hangs on /pods/sparql; rejects when signal aborts. Non-Fuseki fetches return 503.
    const fetchFn = ((url: string, init?: { signal?: AbortSignal }) => {
      if (url.includes('/pods/sparql')) {
        return new Promise((_, reject) => {
          init?.signal?.addEventListener('abort', () => reject(new Error('aborted')));
        });
      }
      return Promise.resolve({ ok: false, status: 503, json: async () => ({}) });
    }) as unknown as FetchFn;
    const p = fetchCrawl('photos', deps({ fetchFn }));
    // sparqlPost is called sequentially up to 4× (collectRdf×2 + collectOwlClassProps + collectOwlRelations);
    // each queues a fresh 5s timer after the previous one resolves. advanceTimersByTimeAsync flushes
    // microtasks between fires so the chain unblocks.
    await jest.advanceTimersByTimeAsync(30_000);
    const r = await p;
    const b = r.body as { rdf?: { triples?: unknown[]; relationships?: unknown[] } };
    expect(b.rdf?.triples ?? []).toEqual([]);
    expect(b.rdf?.relationships ?? []).toEqual([]);
    jest.useRealTimers();
  }, 30_000);

  test('#3091: SLOW Loki on collectLogs aborts at 5s — logs bucket empty, no hang', async () => {
    jest.useFakeTimers();
    // Hangs on the collectLogs URL (Loki `{job=` filter), distinct from spine (`{filename=`).
    const fetchFn = ((url: string, init?: { signal?: AbortSignal }) => {
      if (url.includes('loki') && url.includes('job%3D')) {
        return new Promise((_, reject) => {
          init?.signal?.addEventListener('abort', () => reject(new Error('aborted')));
        });
      }
      return Promise.resolve({ ok: false, status: 503, json: async () => ({}) });
    }) as unknown as FetchFn;
    const p = fetchCrawl('photos', deps({ fetchFn }));
    await jest.advanceTimersByTimeAsync(30_000);
    const r = await p;
    const b = r.body as { logs?: unknown[] };
    expect(b.logs ?? []).toEqual([]);
    jest.useRealTimers();
  }, 30_000);
});

// --- #3606 — OWL bucket coverage: collectOwlClassProps / collectOwlRelations /
// formatOwlProperty paths (lines ~313-352) had no fixture driving them.
describe('fetchCrawl — OWL bucket (#3606)', () => {
  // SPARQL POSTs hit fusekiUrl (not Loki's filename= route). Return class rows
  // for the class query, other-class rows for the relations query.
  function owlFetch(): FetchFn {
    return (async (url: string, opts?: { body?: string }) => {
      // sparqlPost sends application/x-www-form-urlencoded (query=<encoded sparql>)
      const body = decodeURIComponent(String(opts?.body ?? '').replace(/\+/g, ' '));
      if (url.includes('/pods/sparql')) {
        if (body.includes('?class a owl:Class')) {
          return {
            ok: true, status: 200,
            json: async () => ({ results: { bindings: [
              { class: { value: 'https://x#PhotoAsset' }, p: { value: 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type' }, o: { value: 'owl:Class' } },
              { class: { value: 'https://x#PhotoAsset' }, p: { value: 'https://x#hasPath' }, o: { value: '/Volumes/G/' + 'x'.repeat(90) } },
              { p: { value: 'https://x#orphan' }, o: { value: 'no-class-row' } },
            ] } }),
          };
        }
        if (body.includes('?other a owl:Class')) {
          return {
            ok: true, status: 200,
            json: async () => ({ results: { bindings: [
              { other: { value: 'https://x#MusicTrack' } },
              {},
            ] } }),
          };
        }
      }
      return { ok: false, status: 503, json: async () => ({}) };
    }) as unknown as FetchFn;
  }

  it('collects class properties (skipping #type rows and class-less rows) and other-class relations', async () => {
    const r = await fetchCrawl('photos', deps({ fetchFn: owlFetch() }));
    expect(r.status).toBe(200);
    const owl = (r.body as { owl: { properties: string[]; relationships: string[] } }).owl;
    // #type predicate row skipped; long value truncated with ellipsis
    expect(owl.properties).toHaveLength(1);
    expect(owl.properties[0]).toMatch(/^PhotoAsset::hasPath=/);
    expect(owl.properties[0]).toContain('...');
    expect(owl.relationships).toContain('https://x#MusicTrack');
  });
});
