// @test-type: unit — #4605: Clearing v2 room messages reach Chorus search.
// @domain: search — placed by chorus-crawl's domain rules (#4538)
// v2 posts every room message to our Buzz relay as kind 1 (tags: t=team with an
// author tag for room posts, t=reply with a role tag for role replies). The relay
// reader used to read kind 9 only, so v2's room never reached the index (1 row
// since 10-10). What Jeff sees: something said in the room is findable in search,
// under the person who said it. Hermetic: fetchBuzz stubbed, parseBuzzLine fed a
// psql line — no relay, no ssh, no live db.
import { createIndexAllSources } from '../src/index-all-sources';
import { parseBuzzLine } from '../src/index-all-sources-deps';

function fakeDbFactory() {
  const runs: any[] = [];
  const ctor = jest.fn((_p: string) => ({
    pragma: jest.fn(),
    prepare: (sql: string) => ({
      run: (...args: any[]) => { runs.push({ sql, args }); return undefined; },
      get: (..._args: any[]) => undefined,
    }),
    transaction: (fn: (events: any[]) => void) => (events: any[]) => fn(events),
    close: jest.fn(),
  }));
  return { ctor, runs };
}
const EMPTY_FS = {
  existsSync: jest.fn(() => false),
  readdirSync: jest.fn(() => [] as string[]),
  readFileSync: jest.fn(() => ''),
  statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-10-18T00:00:00Z' } })),
};
const base = (over: any = {}) => ({
  dbPath: '/db', fs: EMPTY_FS as any, path: { join: (...p: string[]) => p.join('/') } as any,
  repoRoot: '/repo', homedir: () => '/home', now: () => '2026-10-18T14:40:00Z',
  ...over,
});

const SIGNER = 'f35e29b9a3e89d00476c3d1e56a350c35cc1d37ee508998e7eff99fef15a01e7';
const ROOM = {
  id: '1'.repeat(64), pubkey: SIGNER, author: 'bridge', created_at: '2026-10-18T14:33:00Z',
  kind: 1, channel: 'team', content: 'finish as much of clearing v2 as possible',
  tags: [['t', 'team'], ['author', 'jeff']],
};
const REPLY = {
  id: '2'.repeat(64), pubkey: 'b'.repeat(64), author: 'abby-normal', created_at: '2026-10-18T14:34:00Z',
  kind: 1, channel: 'team', content: '--- Abby Normal | #4604 ---',
  tags: [['t', 'reply'], ['role', 'abby-normal'], ['seq', '714']],
};
const SYSTEM = {
  id: '3'.repeat(64), pubkey: SIGNER, author: 'bridge', created_at: '2026-10-18T14:30:00Z',
  kind: 1, channel: 'team', content: 'Demo ready: #4604', tags: [['t', 'team']],
};
const LEGACY = { ...ROOM, id: '4'.repeat(64), kind: 9, tags: [] as string[][], content: 'kind 9 chat' };

const inserts = (runs: any[], source: string) => runs.filter((x) => x.sql.includes('INTO messages') && x.args[0] === source);

describe('#4605 — Clearing v2 room messages in search', () => {
  it('a room message is indexed as source clearing, under the person who said it', async () => {
    const { ctor, runs } = fakeDbFactory();
    const r = await createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [ROOM] }))();
    expect(r.indexed['clearing-v2']).toBe('1 room messages indexed');
    const [row] = inserts(runs, 'clearing');
    expect(row).toBeDefined();
    // (source, id, channel, role, author, content, ts)
    expect(row.args[1]).toBe(`clearing:${ROOM.id}`);
    expect(row.args[2]).toBe('clearing:team');
    expect(row.args[3]).toBe('jeff');
    expect(row.args[4]).toBe('jeff');
    expect(row.args[5]).toBe(ROOM.content);
    expect(row.args[6]).toBe(ROOM.created_at);
    // negative proof: on the old reader a v2 message was a buzz row (or nothing), never clearing
    expect(inserts(runs, 'buzz')).toHaveLength(0);
  });

  it('a role reply is attributed to the role from its role tag', async () => {
    const { ctor, runs } = fakeDbFactory();
    await createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [REPLY] }))();
    expect(inserts(runs, 'clearing')[0].args[4]).toBe('abby-normal');
  });

  it('a room post with no author tag stays under its signer, not invented', async () => {
    const { ctor, runs } = fakeDbFactory();
    await createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [SYSTEM] }))();
    expect(inserts(runs, 'clearing')[0].args[4]).toBe('bridge');
  });

  it('legacy kind 9 events stay buzz rows; counts are reported per source', async () => {
    const { ctor, runs } = fakeDbFactory();
    const r = await createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [LEGACY, ROOM, REPLY] }))();
    expect(r.indexed.buzz).toBe('1 buzz events indexed');
    expect(r.indexed['clearing-v2']).toBe('2 room messages indexed');
    expect(inserts(runs, 'buzz')).toHaveLength(1);
    expect(inserts(runs, 'clearing')).toHaveLength(2);
  });
});

describe('#4605 — parseBuzzLine reads kind 1 and its tags from the relay row', () => {
  const b64 = (s: string) => Buffer.from(s, 'utf8').toString('base64');
  const line = (kind: number, tags: string) =>
    ['1'.repeat(64), SIGNER, '2026-10-18T14:33:00Z', String(kind), 'team', b64('hello\nroom'), b64(tags)].join('\t');

  it('decodes content and tags', () => {
    const row = parseBuzzLine(line(1, '[["t", "team"], ["author", "jeff"]]'), new Map([[SIGNER, 'bridge']]));
    expect(row).toEqual(expect.objectContaining({
      kind: 1, author: 'bridge', content: 'hello\nroom', tags: [['t', 'team'], ['author', 'jeff']],
    }));
  });

  it('a row without a tags column (old 6-column shape) still parses, tags empty', () => {
    const six = line(9, '[]').split('\t').slice(0, 6).join('\t');
    expect(parseBuzzLine(six, new Map())?.tags).toEqual([]);
  });

  it('garbage tags never throw; the row keeps empty tags', () => {
    expect(parseBuzzLine(line(1, 'not json'), new Map())?.tags).toEqual([]);
  });

  it('a non-event line is skipped', () => {
    expect(parseBuzzLine('(3 rows)', new Map())).toBeNull();
  });
});
