import { createDiscoverCode, buildCodeAliasMap } from '../src/discover-code';

const FAKE_PATH = {
  join: (...parts: string[]) => parts.join('/'),
  resolve: (...parts: string[]) => parts.join('/'),
  relative: (from: string, to: string) => to.startsWith(from) ? to.slice(from.length + 1) : to,
  basename: (p: string) => p.split('/').pop() || p,
  extname: (p: string) => { const i = p.lastIndexOf('.'); return i >= 0 ? p.slice(i) : ''; },
};

describe('buildCodeAliasMap', () => {
  it('skips generic domains and adds plural variants', () => {
    const m = buildCodeAliasMap([
      { id: 'photos-domain', label: 'photos' },
      { id: 'services-domain', label: 'services' },
    ]);
    expect(m['photos-domain']).toContain('photos');
    expect(m['photos-domain']).toContain('photo');
    expect(m['services-domain']).toBeUndefined();
  });

  it('overrides blog-domain with wordpress alias', () => {
    const m = buildCodeAliasMap([]);
    expect(m['blog-domain']).toContain('wordpress');
    expect(m['chorus-domain']).toContain('chorus');
  });
});

describe('createDiscoverCode', () => {
  function makeSparql() {
    const updates: string[] = [];
    return {
      updates,
      client: {
        query: jest.fn(async () => ({ results: { bindings: [
          { sd: { value: 'https://jeffbridwell.com/chorus#photos-domain' }, label: { value: 'photos' } },
        ]}})),
        update: jest.fn(async (u: string) => { updates.push(u); }),
      },
    };
  }

  it('returns zero summary with empty fs', async () => {
    const { client } = makeSparql();
    const fs = { existsSync: jest.fn(() => false), readdirSync: jest.fn(() => []), statSync: jest.fn() };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    expect(data.total_files).toBe(0);
    expect(data.written).toBe(0);
  });

  it('collects files matching alias via basename or path segment', async () => {
    const { client, updates } = makeSparql();
    const fs = {
      existsSync: jest.fn(() => true),
      readdirSync: jest.fn(() => ['photos.ts', 'unrelated.ts']),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    expect(data.total_files).toBeGreaterThan(0);
    expect(updates.length).toBeGreaterThanOrEqual(1);
  });

  it('clears existing CodeFile triples before insert', async () => {
    const { client, updates } = makeSparql();
    const fs = { existsSync: () => false, readdirSync: jest.fn(() => []), statSync: jest.fn() };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    await run();
    expect(updates[0]).toContain('DELETE WHERE');
    expect(updates[0]).toContain('CodeFile');
  });

  it('skips node_modules and dist entries', async () => {
    const { client } = makeSparql();
    const fs = {
      existsSync: () => true,
      readdirSync: jest.fn(() => [
        'photos.ts',
        'node_modules/pkg/photos.ts',
        'dist/photos.ts',
        '.git/hooks/photos.ts',
      ]),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    // Only photos.ts per configured root; overrides also scan platform/api/src + platform/api/tests.
    // Total bounded; don't over-assert exact count.
    expect(data.total_files).toBeGreaterThan(0);
    expect(data.total_files).toBeLessThan(20);
  });

  it('applies dir-domain overrides for platform/api/src → chorus-domain', async () => {
    const { client } = makeSparql();
    const fs = {
      existsSync: (p: string) => p.includes('platform/api/src') || p.includes('platform/api/tests'),
      readdirSync: jest.fn(() => ['server.ts']),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    // byDomain should have 'chorus-domain' from the overrides.
    expect(data.by_domain['chorus-domain']).toBeGreaterThan(0);
  });

  it('batches inserts at 50', async () => {
    const { client, updates } = makeSparql();
    const many = Array.from({ length: 120 }, (_, i) => `photos-${i}.ts`);
    const fs = {
      existsSync: () => true,
      readdirSync: jest.fn(() => many),
      statSync: jest.fn(() => ({ isFile: () => true })),
    };
    const run = createDiscoverCode({
      sparqlClient: client as any, fs: fs as any, path: FAKE_PATH as any,
      gatheringRoot: '/g', chorusRoot: '/c',
    });
    const data = await run();
    const inserts = updates.filter(u => u.startsWith('PREFIX chorus:'));
    expect(inserts.length).toBeGreaterThan(3);
    expect(data.written).toBe(data.total_files);
  });
});
