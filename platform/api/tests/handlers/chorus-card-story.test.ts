/**
 * chorus-card-story handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusCardStory, type ChorusCardStoryDeps, type CardMeta, type NudgeMessage } from '../../src/handlers/chorus-card-story';

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      author TEXT, content TEXT, timestamp TEXT, role TEXT
    );
  `);
  const m = db.prepare('INSERT INTO messages (author, content, timestamp, role) VALUES (?,?,?,?)');
  m.run('user', 'working on #42 today', '2026-04-18T10:00:00', 'wren');
  m.run('assistant', '#42 ready for review', '2026-04-18T11:00:00', 'wren');
  m.run('user', '<system-reminder>about #42</system-reminder>', '2026-04-18T12:00:00', 'wren');
  m.run('user', '#42 x', '2026-04-18T13:00:00', 'wren');
  m.run('user', 'no card ref here', '2026-04-18T14:00:00', 'silas');
  return db;
}

function deps(over: Partial<ChorusCardStoryDeps> = {}): ChorusCardStoryDeps {
  return {
    loadCard: async () => null,
    db: null,
    readLog: () => null,
    loadNudges: async () => [],
    ...over,
  };
}

describe('fetchChorusCardStory (#2188)', () => {
  test('non-numeric id → 400', async () => {
    const r = await fetchChorusCardStory(deps(), 'abc');
    expect(r.status).toBe(400);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/invalid card id/i);
  });

  test('all sources empty → 200 with empty timeline', async () => {
    const r = await fetchChorusCardStory(deps(), '42');
    expect(r.status).toBe(200);
    const body = r.body as { card: number; timeline: unknown[]; count: number; sources: unknown[] };
    expect(body.card).toBe(42);
    expect(body.timeline).toEqual([]);
    expect(body.count).toBe(0);
    expect(body.sources).toEqual([]);
  });

  test('loadCard fills title/owner/status/domain and adds comments', async () => {
    const card: CardMeta = {
      title: 'Fix the thing',
      owner: 'Wren',
      status: 'Done',
      created: '2026-04-18T09:00:00',
      domains: ['domain:chorus', 'type:fix'],
      comments: [
        { text: 'did the fix', created: '2026-04-18T09:30:00', author: 'wren' },
        { text: 'ok', author: 'wren' }, // too short — filtered
      ],
    };
    const body = (await fetchChorusCardStory(deps({ loadCard: async () => card }), '42')).body as {
      title: string; owner: string; status: string; domain: string;
      timeline: Array<{ source: string; text: string }>;
    };
    expect(body.title).toBe('Fix the thing');
    expect(body.owner).toBe('wren');
    expect(body.status).toBe('Done');
    expect(body.domain).toBe('chorus');
    expect(body.timeline.length).toBe(1);
    expect(body.timeline[0].source).toBe('vikunja');
  });

  test('loadCard throwing is swallowed', async () => {
    const r = await fetchChorusCardStory(deps({ loadCard: async () => { throw new Error('boom'); } }), '42');
    expect(r.status).toBe(200);
  });

  test('db mentions included with user→jeff mapping, filters applied', async () => {
    const db = seedDb();
    const body = (await fetchChorusCardStory(deps({ db }), '42')).body as {
      timeline: Array<{ source: string; text: string; role?: string }>;
    };
    // 2 of 5 rows pass filter (#42 mentions, >10 char, not system-reminder)
    const mentions = body.timeline.filter((t) => t.source === 'chorus-index');
    expect(mentions.length).toBe(2);
    expect(mentions[0].role).toBe('jeff'); // author=user → jeff
    db.close();
  });

  test('spine log events included for matching card= or "card":"N"', async () => {
    const log = [
      JSON.stringify({ event: 'card.moved.to.WIP', card: '42', role: 'wren', timestamp: '2026-04-18T08:00:00' }),
      JSON.stringify({ event: 'other.event', card: '42', role: 'wren', timestamp: '2026-04-18T09:00:00' }),
      JSON.stringify({ event: 'card.pulled', timestamp: '2026-04-18T10:00:00', role: 'silas' }) + ' card=99',
      'malformed-line',
    ].join('\n');
    const body = (await fetchChorusCardStory(deps({ readLog: () => log }), '42')).body as {
      timeline: Array<{ source: string; text: string }>;
    };
    const spine = body.timeline.filter((t) => t.source === 'spine');
    expect(spine.length).toBe(1);
    expect(spine[0].text).toBe('card.moved.to.WIP');
  });

  test('nudges filtered to ones mentioning #cardId', async () => {
    const nudges: NudgeMessage[] = [
      { from: 'silas', to: 'wren', text: 'hey #42 is ready', timestamp: '2026-04-18T07:00:00' },
      { from: 'kade', to: 'wren', text: 'unrelated thing', timestamp: '2026-04-18T08:00:00' },
      { from: 'silas', to: 'wren', text: '#99 not this one', timestamp: '2026-04-18T09:00:00' },
    ];
    const body = (await fetchChorusCardStory(deps({ loadNudges: async () => nudges }), '42')).body as {
      timeline: Array<{ source: string; role?: string }>;
    };
    const nudgeEntries = body.timeline.filter((t) => t.source === 'nudge');
    expect(nudgeEntries.length).toBe(1);
    expect(nudgeEntries[0].role).toBe('silas');
  });

  test('timeline ordered by timestamp ASC across sources', async () => {
    const card: CardMeta = {
      title: 'T', comments: [{ text: 'late comment', created: '2026-04-18T23:00:00', author: 'wren' }],
    };
    const nudges: NudgeMessage[] = [
      { from: 'silas', to: 'wren', text: 'early #42', timestamp: '2026-04-18T01:00:00' },
    ];
    const body = (await fetchChorusCardStory(
      deps({ loadCard: async () => card, loadNudges: async () => nudges }),
      '42',
    )).body as { timeline: Array<{ timestamp: string }> };
    expect(body.timeline.map((t) => t.timestamp)).toEqual(['2026-04-18T01:00:00', '2026-04-18T23:00:00']);
  });

  test('sources dedup from timeline', async () => {
    const db = seedDb();
    const card: CardMeta = { title: 'T', comments: [{ text: 'a very long comment here', created: '2026-04-18T01:00:00', author: 'wren' }] };
    const body = (await fetchChorusCardStory(deps({ db, loadCard: async () => card }), '42')).body as {
      sources: string[];
    };
    expect(new Set(body.sources)).toEqual(new Set(['vikunja', 'chorus-index']));
    db.close();
  });
});
