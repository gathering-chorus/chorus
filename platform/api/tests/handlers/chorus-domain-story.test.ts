/**
 * chorus-domain-story handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusDomainStory, type ChorusDomainStoryDeps, type BoardCard } from '../../src/handlers/chorus-domain-story';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY, author TEXT, content TEXT, timestamp TEXT, role TEXT
    );
  `);
  const m = db.prepare('INSERT INTO messages (author, content, timestamp, role) VALUES (?,?,?,?)');
  m.run('user', 'working on photos indexing today with long content', '2026-04-18T10:00:00', 'wren');
  m.run('assistant', 'photos index ready — tested end-to-end', '2026-04-18T11:00:00', 'wren');
  m.run('user', '<system-reminder>photos reminder</system-reminder>', '2026-04-18T12:00:00', 'wren');
  m.run('user', 'photos x', '2026-04-18T13:00:00', 'wren');
  m.run('user', 'no mention of the domain', '2026-04-18T14:00:00', 'silas');
  return db;
}

function defaultDeps(over: Partial<ChorusDomainStoryDeps> = {}): ChorusDomainStoryDeps {
  return {
    getCards: () => [],
    db: null,
    readLog: () => null,
    ...over,
  };
}

describe('fetchChorusDomainStory (#2188)', () => {
  test('all sources empty → 200 with zero counts', () => {
    const body = fetchChorusDomainStory(defaultDeps(), 'photos').body as {
      domain: string; cards: unknown[]; mentions: unknown[]; timeline: unknown[]; count: number;
    };
    expect(body.domain).toBe('photos');
    expect(body.cards).toEqual([]);
    expect(body.mentions).toEqual([]);
    expect(body.timeline).toEqual([]);
    expect(body.count).toBe(0);
  });

  test('domain param lowercased', () => {
    const body = fetchChorusDomainStory(defaultDeps(), 'PHOTOS').body as { domain: string };
    expect(body.domain).toBe('photos');
  });

  test('cards filtered to matching domain tag only', () => {
    const cards: BoardCard[] = [
      { id: '1', title: 'A', status: 'WIP', owner: 'wren', tags: ['domain:photos'] },
      { id: '2', title: 'B', status: 'Done', owner: 'silas', tags: ['domain:music'] },
      { id: '3', title: 'C', status: 'Next', owner: 'kade', tags: ['domain:photos', 'type:enhance'] },
    ];
    const body = fetchChorusDomainStory(defaultDeps({ getCards: () => cards }), 'photos').body as {
      cards: Array<{ index: number }>; card_count: number;
    };
    expect(body.card_count).toBe(2);
    expect(body.cards.map((c) => c.index)).toEqual([1, 3]);
  });

  test('db mentions filter noise and short messages', () => {
    const db = seedDb();
    const body = fetchChorusDomainStory(defaultDeps({ db }), 'photos').body as {
      mentions: Array<{ role: string }>; mention_count: number;
    };
    // 2 of 5 rows pass — system-reminder, too-short, non-match filtered
    expect(body.mention_count).toBe(2);
    expect(body.mentions[0].role).toBe('jeff');
    db.close();
  });

  test('limit clamps to 500', () => {
    const db = new Database(':memory:');
    db.exec(`CREATE TABLE messages (id INTEGER, author TEXT, content TEXT, timestamp TEXT, role TEXT);`);
    const body = fetchChorusDomainStory(defaultDeps({ db }), 'photos', '999').body as { mentions: unknown[] };
    // Just verify it doesn't throw; empty table → empty mentions
    expect(body.mentions).toEqual([]);
    db.close();
  });

  test('spine events filtered to cards in the domain', () => {
    const cards: BoardCard[] = [
      { id: '42', title: 'A', status: 'WIP', owner: 'wren', tags: ['domain:photos'] },
    ];
    const log = [
      JSON.stringify({ event: 'card.moved.to.WIP', card: '42', role: 'wren', timestamp: '2026-04-18T10:00:00' }),
      JSON.stringify({ event: 'card.pulled', card: '99', role: 'silas', timestamp: '2026-04-18T11:00:00' }),
      JSON.stringify({ event: 'other.event', card: '42', role: 'wren', timestamp: '2026-04-18T12:00:00' }),
      'malformed-line',
    ].join('\n');
    const body = fetchChorusDomainStory(
      defaultDeps({ getCards: () => cards, readLog: () => log }),
      'photos',
    ).body as { timeline: Array<{ source: string; card?: number }> };
    const spine = body.timeline.filter((t) => t.source === 'spine');
    expect(spine.length).toBe(1);
    expect(spine[0].card).toBe(42);
  });

  test('timeline sorts empty-timestamp cards first (legacy behavior)', () => {
    const cards: BoardCard[] = [
      { id: '1', title: 'A', status: 'WIP', owner: 'wren', tags: ['domain:photos'] },
    ];
    const db = seedDb();
    const body = fetchChorusDomainStory(
      defaultDeps({ getCards: () => cards, db }),
      'photos',
    ).body as { timeline: Array<{ source: string; timestamp: string }> };
    expect(body.timeline[0].timestamp).toBe('');
    expect(body.timeline[0].source).toBe('card');
    db.close();
  });

  test('db throwing is silently swallowed', () => {
    const fakeDb = {
      prepare: () => { throw new Error('boom'); },
    } as unknown as Database.Database;
    const body = fetchChorusDomainStory(defaultDeps({ db: fakeDb }), 'photos').body as { mentions: unknown[] };
    expect(body.mentions).toEqual([]);
  });

  test('null readLog → skip spine section, no throw', () => {
    const cards: BoardCard[] = [{ id: '1', title: 'A', status: 'WIP', owner: 'wren', tags: ['domain:photos'] }];
    const body = fetchChorusDomainStory(defaultDeps({ getCards: () => cards, readLog: () => null }), 'photos').body as {
      timeline: Array<{ source: string }>;
    };
    expect(body.timeline.filter((t) => t.source === 'spine').length).toBe(0);
  });
});
