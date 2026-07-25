// @test-type: unit — #3695 AC3: Buzz events (our self-hosted relay) enter the
// Chorus index with Principal attribution. Hermetic: fetchBuzz stubbed (the
// graph-sources pattern) — no relay, no ssh, no live db. What Jeff sees:
// a signed relay message is findable via chorus search, attributed to a
// Principal, and a relay outage never tanks the other sources.
import { createIndexAllSources } from '../src/index-all-sources';

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
const NEVER = () => false;
const EMPTY_FS = {
  existsSync: jest.fn(NEVER),
  readdirSync: jest.fn(() => [] as string[]),
  readFileSync: jest.fn(() => ''),
  statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-07-25T00:00:00Z' } })),
};
const FAKE_PATH = { join: (...parts: string[]) => parts.join('/') };
const base = (over: any = {}) => ({
  dbPath: '/db', fs: EMPTY_FS as any, path: FAKE_PATH as any,
  repoRoot: '/repo', homedir: () => '/home', now: () => '2026-07-25T00:00:00Z',
  ...over,
});

const EV = {
  id: 'e7e3feba81898df8192a524f2203de62314572e678926b706007ce152a043113',
  pubkey: 'f35e29b9a3e89d00476c3d1e56a350c35cc1d37ee508998e7eff99fef15a01e7',
  author: 'bridge',                       // deps resolved pubkey → Principal
  created_at: '2026-07-25T19:13:21Z',
  kind: 9,
  channel: 'team',
  content: 'c3 proof: bridge-signed as principal-bridge',
};

describe('#3695 AC3 — buzz source', () => {
  it('indexes a relay event with Principal attribution (author + role = the Principal)', async () => {
    const { ctor, runs } = fakeDbFactory();
    const run = createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [EV] }));
    const r = await run();
    expect(r.indexed.buzz).toBe('1 buzz events indexed');
    const row = runs.find((x) => x.sql.includes('INSERT') && x.args[0] === 'buzz');
    expect(row).toBeDefined();
    // (source, id, channel, role, author, content, ts) — the house column shape
    expect(row.args[1]).toBe(`buzz:${EV.id}`);
    expect(row.args[2]).toBe('buzz:team');
    expect(row.args[3]).toBe('bridge');
    expect(row.args[4]).toBe('bridge');
    expect(row.args[5]).toContain('bridge-signed');
    expect(row.args[6]).toBe(EV.created_at);
  });

  it('an unmapped pubkey stays VISIBLE as unattributed, never silently dropped or misattributed', async () => {
    const { ctor, runs } = fakeDbFactory();
    const stray = { ...EV, id: 'a'.repeat(64), pubkey: 'b'.repeat(64), author: '' };
    const run = createIndexAllSources(base({ DatabaseCtor: ctor as any, fetchBuzz: async () => [stray] }));
    const r = await run();
    expect(r.indexed.buzz).toBe('1 buzz events indexed');
    const row = runs.find((x) => x.sql.includes('INSERT') && x.args[0] === 'buzz');
    expect(row.args[3]).toBe('unattributed:bbbbbbbb'); // pubkey prefix — auditable, not fabricated
  });

  it('relay down = source error, other sources unharmed (one failure never tanks the loop)', async () => {
    const { ctor } = fakeDbFactory();
    const run = createIndexAllSources(base({
      DatabaseCtor: ctor as any,
      fetchBuzz: async () => { throw new Error('relay unreachable'); },
    }));
    const r = await run();
    expect(String(r.indexed.buzz)).toContain('error');
    expect(r.indexed.briefs).toBe('0 briefs indexed'); // the loop survived
  });

  it('no fetchBuzz dep = source absent (deploys without the relay stay unchanged)', async () => {
    const { ctor } = fakeDbFactory();
    const run = createIndexAllSources(base({ DatabaseCtor: ctor as any }));
    const r = await run();
    expect(r.indexed.buzz).toBeUndefined();
  });
});
