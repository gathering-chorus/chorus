import { createIndexAllSources } from '../src/index-all-sources';

function fakeDbFactory() {
  const runs: any[] = [];
  const ctor = jest.fn((_p: string) => ({
    pragma: jest.fn(),
    prepare: (sql: string) => ({
      run: (...args: any[]) => { runs.push({ sql, args }); return undefined; },
      // #3077 AC2: indexSpine reads watermarks('spine:offset') via .get(); undefined
      // here → offset 0 → readSince fallback reads the whole file (prior behavior).
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
  statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-04-19T00:00:00Z' } })),
};
const FAKE_PATH = { join: (...parts: string[]) => parts.join('/') };

describe('createIndexAllSources', () => {
  it('opens + closes the db and returns an elapsed_ms number with no sources present', async () => {
    const { ctor } = fakeDbFactory();
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: EMPTY_FS as any, path: FAKE_PATH as any,
      repoRoot: '/repo', homedir: () => '/home',
      now: () => '2026-04-19T00:00:00Z',
    });
    const r = await run();
    expect(ctor).toHaveBeenCalledWith('/db');
    expect(typeof r.elapsed_ms).toBe('number');
    // Sources with unconditional watermark writes emit a '0 X indexed' even on
    // empty fs. Sources with existsSync-guarded bodies stay absent.
    expect(r.indexed.slack).toBe('removed (deprecated)');
    expect(r.indexed.briefs).toBe('0 briefs indexed');
    expect(r.indexed.state).toBe('0 state files indexed');
  });

  it('indexes spine events when the log file exists', async () => {
    const log = [
      JSON.stringify({ timestamp: '2026-04-19T00:00:00Z', event: 'card.pulled', role: 'kade' }),
      JSON.stringify({ timestamp: '2026-04-19T00:01:00Z', event: 'card.accepted' }),
      'malformed-json-line',
    ].join('\n');
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('chorus.log'),
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => log),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-04-19T00:00:00Z' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.spine).toMatch(/\d+ events indexed/);
    // Filter by the INSERT INTO messages sql shape (7 args) to exclude the
    // updateWatermark.run('spine', ...) call which also uses first-arg 'spine'.
    const spineInserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'spine');
    expect(spineInserts.length).toBe(2); // malformed line was skipped
  });

  it('indexes brief files per role when the brief dir exists', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('/briefs'),
      readdirSync: jest.fn((p: string) => {
        if (p.endsWith('wren/briefs')) return ['a.md', 'b.md', '.hidden.md', 'note.txt'];
        return [];
      }),
      readFileSync: jest.fn(() => 'brief content'),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-04-19T00:00:00Z' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.briefs).toBe('2 briefs indexed');
    const briefInserts = runs.filter(e => e.args[0] === 'brief');
    expect(briefInserts).toHaveLength(2);
  });

  it('parses decisions.md into DEC-prefixed decision records', async () => {
    // Content starts with `## DEC-` sentinel so split yields only real entries.
    const decContent = `## DEC-001 First\nbody one\n## DEC-042 Second\nbody two`;
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('decisions.md'),
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => decContent),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    // Split pattern is '\n## DEC-' — the first entry before that sentinel is
    // always recorded. With a leading sentinel, content is ['## DEC-001 First\nbody one']
    // joined with the split piece... actually JS split doesn't preserve the
    // delimiter. Here '## DEC-001 First\nbody one\n## DEC-042 Second\nbody two'
    // split on '\n## DEC-' yields ['## DEC-001 First\nbody one', '042 Second\nbody two'].
    // Two entries, matching the count.
    expect(r.indexed.decisions).toBe('2 decisions indexed');
    const decInserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'decision');
    expect(decInserts).toHaveLength(2);
  });

  it('indexes ADR files from the silas/adr directory', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('/silas/adr'),
      readdirSync: jest.fn(() => ['ADR-001.md', 'ADR-002.md']),
      readFileSync: jest.fn(() => 'adr body'),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.adrs).toBe('2 ADRs indexed');
    expect(runs.filter(e => e.args[0] === 'adr')).toHaveLength(2);
  });

  it('indexes activity.md when it exists', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('activity.md'),
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => 'activity body'),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.activity).toBe('indexed');
    expect(runs.filter(e => e.args[0] === 'activity')).toHaveLength(1);
  });

  it('traverses memory dirs containing "chorus" under homedir/.claude/projects', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.includes('.claude/projects') || p.endsWith('/memory'),
      readdirSync: jest.fn((p: string) => {
        if (p.endsWith('.claude/projects')) return ['chorus-kade', 'other-proj'];
        if (p.endsWith('/memory')) return ['MEMORY.md', 'a.md'];
        return [];
      }),
      readFileSync: jest.fn(() => 'memory body'),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.memory).toBe('2 memory files indexed');
    expect(runs.filter(e => e.args[0] === 'memory')).toHaveLength(2);
  });

  it('records per-source error strings when an indexer throws', async () => {
    const { ctor } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('chorus.log'),
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => { throw new Error('disk fail'); }),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.spine).toContain('error:');
    expect(r.indexed.spine).toContain('disk fail');
  });

  it('removes deprecated slack watermarks as the final indexer', async () => {
    const { ctor, runs } = fakeDbFactory();
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: EMPTY_FS as any, path: FAKE_PATH as any,
      repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.slack).toBe('removed (deprecated)');
    const slackSqls = runs.filter(e => e.sql.includes('DELETE FROM watermarks'));
    expect(slackSqls.length).toBeGreaterThanOrEqual(2);
  });
});
