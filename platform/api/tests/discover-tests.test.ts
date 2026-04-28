import { createDiscoverTests, classifyTestType, inferDomain, loadAliasMap } from '../src/discover-tests';

const FAKE_PATH = {
  join: (...parts: string[]) => parts.join('/'),
  resolve: (...parts: string[]) => parts.join('/'),
  relative: (from: string, to: string) => to.startsWith(from) ? to.slice(from.length + 1) : to,
  basename: (p: string) => p.split('/').pop() || p,
};

describe('classifyTestType', () => {
  it('detects e2e by path and by extension', () => {
    expect(classifyTestType('tests/e2e/app.spec.ts')).toBe('e2e');
    expect(classifyTestType('tests/app.e2e.ts')).toBe('e2e');
  });

  it('detects integration, performance, security path conventions', () => {
    expect(classifyTestType('tests/integration/x.ts')).toBe('integration');
    expect(classifyTestType('tests/performance/x.ts')).toBe('performance');
    expect(classifyTestType('tests/security/x.ts')).toBe('security');
  });

  it('classifies .bats and .feature as bdd', () => {
    expect(classifyTestType('scripts/check.bats')).toBe('bdd');
    expect(classifyTestType('features/login.feature')).toBe('bdd');
  });

  it('defaults to unit', () => {
    expect(classifyTestType('tests/unit/foo.test.ts')).toBe('unit');
  });
});

describe('loadAliasMap', () => {
  // Derivation logic lives in scripts/migrate-aliases-to-graph.ts (deriveAliases).
  // This function is the runtime read: take alias triples from SPARQL,
  // produce the prefix → subdomainId map. Trivial — the value is in the
  // graph data, not the function.
  const sdUri = (id: string) => ({ value: `https://jeffbridwell.com/chorus#${id}` });
  const prefix = (s: string) => ({ value: s });

  it('converts alias triples to a prefix → id map', () => {
    const map = loadAliasMap([
      { sd: sdUri('photos-domain'), prefix: prefix('photos') },
      { sd: sdUri('photos-domain'), prefix: prefix('photo') },
      { sd: sdUri('blog-domain'), prefix: prefix('wordpress') },
    ]);
    expect(map.photos).toBe('photos-domain');
    expect(map.photo).toBe('photos-domain');
    expect(map.wordpress).toBe('blog-domain');
  });

  it('last-write-wins on prefix collision (resolution by SPARQL ORDER BY ?sd)', () => {
    // properties-domain plural-folds 'properties' → 'property'; property-domain
    // base = 'property'. ORDER BY ?sd alphabetically puts properties-domain
    // first, property-domain second; last write wins.
    const map = loadAliasMap([
      { sd: sdUri('properties-domain'), prefix: prefix('property') },
      { sd: sdUri('property-domain'), prefix: prefix('property') },
    ]);
    expect(map.property).toBe('property-domain');
  });

  it('skips bindings with empty id or prefix', () => {
    const map = loadAliasMap([
      { sd: sdUri('photos-domain'), prefix: prefix('photos') },
      { sd: { value: '' }, prefix: prefix('orphan') },
      { sd: sdUri('blog-domain'), prefix: { value: '' } },
    ]);
    expect(map.photos).toBe('photos-domain');
    expect(map.orphan).toBeUndefined();
    expect(Object.keys(map)).toHaveLength(1);
  });

  it('returns empty map for empty input', () => {
    expect(loadAliasMap([])).toEqual({});
  });
});

describe('inferDomain', () => {
  it('matches on basename containing an alias', () => {
    const aliases = { photos: 'photos-domain', notes: 'notes-domain' };
    expect(inferDomain('tests/handlers/photos.test.ts', aliases, FAKE_PATH)).toBe('photos-domain');
  });

  it('matches when a path segment equals the alias', () => {
    const aliases = { chorus: 'chorus-domain' };
    expect(inferDomain('tests/chorus/foo.ts', aliases, FAKE_PATH)).toBe('chorus-domain');
  });

  it('returns null when no alias matches', () => {
    expect(inferDomain('tests/foo.ts', { chorus: 'chorus-domain' }, FAKE_PATH)).toBeNull();
  });
});

describe('createDiscoverTests', () => {
  // #2516: query now fetches alias triples (?sd ?prefix) instead of
  // SubDomains (?sd ?label). Mock shape updated.
  function makeSparql(aliasRows?: any[]) {
    const updates: string[] = [];
    const rows = aliasRows ?? [
      { sd: { value: 'https://jeffbridwell.com/chorus#photos-domain' }, prefix: { value: 'photos' } },
      { sd: { value: 'https://jeffbridwell.com/chorus#photos-domain' }, prefix: { value: 'photo' } },
    ];
    return {
      updates,
      client: {
        query: jest.fn(async () => ({ results: { bindings: rows } })),
        update: jest.fn(async (u: string) => { updates.push(u); }),
      },
    };
  }

  it('returns zero summary when no test files found', async () => {
    const { client } = makeSparql();
    const fs = {
      existsSync: jest.fn(() => false),
      readdirSync: jest.fn(() => []),
      statSync: jest.fn(),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    expect(data.total_tests).toBe(0);
    expect(data.written).toBe(0);
  });

  it('calls existsSync for each configured scan root', async () => {
    const { client } = makeSparql();
    const calls: string[] = [];
    const fs = {
      existsSync: (p: string) => { calls.push(p); return false; },
      readdirSync: jest.fn(() => []),
      statSync: jest.fn(),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    await run();
    // #2515: scan roots include cards/tests + platform/tests beyond the original 5
    expect(calls).toContain('/g/tests');
    expect(calls).toContain('/c/platform/api/tests');
    expect(calls).toContain('/c/platform/services/chorus-hooks/tests');
    expect(calls).toContain('/c/proving');
    expect(calls).toContain('/c/docs/diagrams');
    expect(calls).toContain('/c/directing/products/cards/tests');
    expect(calls).toContain('/c/platform/tests');
  });

  it('collects test entries for files matching the extension regex', async () => {
    const { client, updates } = makeSparql();
    const fs = {
      existsSync: (_p: string) => true,
      readdirSync: jest.fn((_p: string) => ['photos.test.ts', 'notes.spec.ts', 'README.md']),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    expect(data.total_tests).toBeGreaterThan(0);
    expect(updates.length).toBeGreaterThanOrEqual(1);
  });

  it('clears existing TestCoverage triples before batch insert', async () => {
    const { client, updates } = makeSparql();
    const fs = {
      existsSync: (_p: string) => false,
      readdirSync: jest.fn(() => []),
      statSync: jest.fn(),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    await run();
    expect(updates[0]).toContain('DELETE WHERE');
    expect(updates[0]).toContain('TestCoverage');
  });

  it('skips entries that fall inside node_modules / .git / dist', async () => {
    const { client } = makeSparql();
    const fs = {
      existsSync: (_p: string) => true,
      readdirSync: jest.fn(() => [
        'photos.test.ts',
        'node_modules/pkg/photos.test.ts',
        '.git/hooks/photos.test.ts',
        'dist/photos.test.ts',
      ]),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    // Only photos.test.ts per root is valid → ≤7 across 7 roots (#2515).
    expect(data.total_tests).toBeLessThanOrEqual(7);
  });

  it('batches inserts in groups of ≤50', async () => {
    const { client, updates } = makeSparql();
    const many = Array.from({ length: 120 }, (_, i) => `photos-${i}.test.ts`);
    const fs = {
      existsSync: (_p: string) => true,
      readdirSync: jest.fn(() => many),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverTests({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    const inserts = updates.filter(u => u.startsWith('PREFIX chorus:'));
    expect(inserts.length).toBeGreaterThanOrEqual(12);
    expect(data.written).toBe(data.total_tests);
  });
});
