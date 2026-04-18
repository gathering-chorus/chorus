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
  db.exec(`CREATE TABLE messages (id INTEGER PRIMARY KEY, author TEXT, content TEXT, role TEXT, timestamp TEXT);`);
  return db;
}

const nullFetch: FetchFn = async () => ({ ok: false, status: 503, json: async () => ({}) });
const nullExec: ExecAsyncFn = async () => ({ stdout: '' });
const nullSparql: AthenaSparqlFn = async () => ({ results: { bindings: [] } });

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
    ].join('\n');
    const r = await fetchCrawl('photos', deps({
      getBoardCards: () => boardCards,
      exists: (p) => p === '/fake/chorus.log',
      readFile: () => logLines,
    }));
    const b = r.body as { spine: Array<{ event: string }> };
    expect(b.spine).toHaveLength(1);
    expect(b.spine[0].event).toBe('card.accepted');
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
});
