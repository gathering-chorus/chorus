// @test-type: unit — fake db + fake fs; no live index.db, no filesystem, brings its own world.
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

  it('stamps the clearing watermark when /tmp/chorus-chat is absent — checked-and-empty is not dead (#3606)', async () => {
    // /tmp/chorus-chat is ephemeral (wiped on reboot). Before #3606 the guard
    // returned without touching the watermark, so `clearing` froze at its last
    // pre-reboot stamp and the freshness endpoint reported it dead forever —
    // the #1960 integration red (11 days dead as of 2026-07-03).
    const { ctor, runs } = fakeDbFactory();
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: EMPTY_FS as any, path: FAKE_PATH as any,
      repoRoot: '/repo', homedir: () => '/home',
      now: () => '2026-04-19T00:00:00Z',
    });
    const r = await run();
    expect(r.indexed.clearing).toBe('0 transcripts indexed (chat dir absent)');
    const wm = runs.filter(e => e.sql.includes('INSERT INTO watermarks') && e.args[0] === 'clearing');
    expect(wm.length).toBe(1);
  });

  it('indexes clearing transcripts when the chat dir exists (#3606 — the populated path)', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p === '/tmp/chorus-chat',
      readdirSync: jest.fn((p: string) => (p === '/tmp/chorus-chat' ? ['sess-1.md', 'notes.txt'] : [])),
      readFileSync: jest.fn(() => 'transcript body'),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => '2026-07-03T12:00:00Z' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any,
      fs: fs as any, path: FAKE_PATH as any,
      repoRoot: '/repo', homedir: () => '/home',
      now: () => '2026-07-03T12:00:00Z',
    });
    const r = await run();
    expect(r.indexed.clearing).toBe('1 transcripts indexed'); // .md only, .txt skipped
    const inserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'clearing');
    expect(inserts).toHaveLength(1);
    expect(inserts[0].args[1]).toBe('clearing:sess-1.md');
    const wm = runs.filter(e => e.sql.includes('INSERT INTO watermarks') && e.args[0] === 'clearing');
    expect(wm.length).toBe(1);
  });

  it('does NOT index spine even when the log exists — telemetry lives in Loki (#3136)', async () => {
    // #3136 REMOVE — spine was 82% of the corpus and the search-latency floor; it is
    // telemetry, queried by trace/card/time via Loki, never by meaning. Removal means
    // zero spine rows regardless of the log's presence or content.
    const log = [
      JSON.stringify({ timestamp: '2026-04-19T00:00:00Z', event: 'card.pulled', role: 'kade' }),
      JSON.stringify({ timestamp: '2026-04-19T00:01:00Z', event: 'card.accepted' }),
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
    expect(r.indexed.spine).toBe('spine NOT indexed — telemetry lives in Loki (#3136)');
    const spineInserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'spine');
    expect(spineInserts.length).toBe(0); // a full log present, yet zero spine rows
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
    // Retargeted off spine (#3136 neutered it) onto a file-reading source: a failing
    // read in indexDecisions must be caught and recorded, not tank the whole reindex.
    const { ctor } = fakeDbFactory();
    const fs = {
      existsSync: (p: string) => p.endsWith('decisions.md'),
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
    expect(r.indexed.decisions).toContain('error:');
    expect(r.indexed.decisions).toContain('disk fail');
  });

  it('indexes doc-catalog content (full scan) with tags stripped (#3136 REFINE)', async () => {
    const { ctor, runs } = fakeDbFactory();
    const html = '<style>.x{color:red}</style><h1>Version Control Design</h1><script>boot()</script><p>Atomic verbs compose</p>';
    const fs = {
      existsSync: (p: string) => p === '/docs/vc.html',
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => html),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    // listDocs is the full SOURCE_DIRS scan (402 docs in prod); stubbed to one here.
    const listDocs = jest.fn(() => ([{ href: '/version-control-service-design.html', title: 'Version Control Design', group: 'Architecture', absPath: '/docs/vc.html' }]));
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: fs as any, path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h',
      listDocs: listDocs as any,
    });
    const r = await run();
    expect(r.indexed.docs).toBe('1 docs indexed');
    const doc = runs.find(e => e.sql.includes('INTO messages') && e.args[0] === 'doc');
    expect(doc!.args[5]).toContain('Version Control Design'); // title + prose kept
    expect(doc!.args[5]).toContain('Atomic verbs compose');
    expect(doc!.args[5]).not.toContain('color:red'); // <style> stripped
    expect(doc!.args[5]).not.toContain('boot()'); // <script> stripped
  });

  it('indexes athena tree products/domains/services (#3136 REFINE)', async () => {
    const { ctor, runs } = fakeDbFactory();
    const tree = JSON.stringify({
      products: [{ iri: 'chorus:loom', label: 'Loom', comment: 'Knowledge layer', ownedBy: 'chorus:role-wren', gaps: [] }],
      domains: [{ iri: 'chorus:roles', label: 'roles', comment: 'identities', gaps: ['no lifecycle'] }],
      services: [],
    });
    const fs = {
      existsSync: (p: string) => p.endsWith('tree.json'),
      readdirSync: jest.fn(() => []),
      readFileSync: jest.fn(() => tree),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: fs as any, path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed.domains).toBe('2 domains indexed');
    const rows = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'domain');
    expect(rows).toHaveLength(2);
    expect(rows[0].args[5]).toContain('Loom (product)');
    expect(rows[0].args[5]).toContain('owner: chorus:role-wren');
    expect(rows.some(x => x.args[5].includes('gaps: no lifecycle'))).toBe(true);
  });

  it('indexes prefetched graph knowledge into principle/policy/practice (#3136 REFINE)', async () => {
    const { ctor, runs } = fakeDbFactory();
    const fs = {
      existsSync: () => false, readdirSync: jest.fn(() => []), readFileSync: jest.fn(() => ''),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const fetchGraph = jest.fn(async () => ([
      { source: 'principle', id: 'principle:observe', content: 'Observe\nprotracted observation' },
      { source: 'policy', id: 'policy:heartbeat', content: '60s heartbeat' },
      { source: 'practice', id: 'practice:actor-bdd', content: 'Actor-BDD\nmodel actors first' },
    ]));
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: fs as any, path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h',
      fetchGraph: fetchGraph as any,
    });
    const r = await run();
    expect(r.indexed.principles).toBe('1 principle indexed');
    expect(r.indexed.policies).toBe('1 policy indexed');
    expect(r.indexed.practices).toBe('1 practice indexed');
    const graph = runs.filter(e => e.sql.includes('INTO messages') && ['principle', 'policy', 'practice'].includes(e.args[0]));
    expect(graph).toHaveLength(3);
  });

  it('coverage signal WARNs when a knowledge source is empty (#3136)', async () => {
    const { ctor } = fakeDbFactory();
    const fs = {
      existsSync: () => false, readdirSync: jest.fn(() => []), readFileSync: jest.fn(() => ''),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: fs as any, path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h',
    });
    const r = await run();
    expect(r.indexed._coverage).toContain('WARN');
    expect(r.indexed._coverage).toContain('docs'); // docs came back empty → flagged
  });

  it('coverage signal reports ok when sources are populated (#3136)', async () => {
    const { ctor } = fakeDbFactory();
    // Every knowledge source present: briefs/decisions/adrs/memory dirs + docs + tree.
    const tree = JSON.stringify({ products: [{ iri: 'chorus:loom', label: 'Loom', comment: 'c', gaps: [] }], domains: [], services: [] });
    const fs = {
      existsSync: () => true,
      readdirSync: jest.fn((p: string) => {
        if (p.endsWith('.claude/projects')) return ['chorus-x'];
        if (p.endsWith('memory')) return ['m.md'];
        if (p.endsWith('/briefs')) return ['b.md'];
        if (p.endsWith('/adr')) return ['a.md'];
        return [];
      }),
      readFileSync: jest.fn((p: string) => {
        if (p.endsWith('tree.json')) return tree;
        if (p.endsWith('decisions.md')) return '## DEC-001 x\nbody';
        return 'content';
      }),
      statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
    };
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: fs as any, path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h',
      listDocs: () => ([{ href: '/a.html', title: 'A', absPath: '/d/a.html' }]),
    });
    const r = await run();
    expect(r.indexed._coverage).toContain('ok');
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
