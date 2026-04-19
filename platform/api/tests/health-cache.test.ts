import { createHealthCache, HealthSnapshot } from '../src/health-cache';

function fakeDb(rows: number, unembedded?: number) {
  return {
    prepare: (sql: string) => ({
      get: () => {
        if (sql.includes('embedded = 0')) {
          if (unembedded === undefined) throw new Error('no column');
          return { cnt: unembedded };
        }
        return { cnt: rows };
      },
    }),
    close: jest.fn(),
  };
}

describe('createHealthCache — snapshot', () => {
  it('returns the initial-zero snapshot before any refresh', () => {
    const hc = createHealthCache({
      dbPath: '/db',
      DatabaseCtor: jest.fn(),
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/hooks',
    });
    const s = hc.snapshot();
    expect(s.dbRows).toBe(0);
    expect(s.dbStatus).toBe('unknown');
    expect(s.ts).toBe(0);
  });
});

describe('createHealthCache — refresh', () => {
  it('populates dbRows + dbStatus=ok when the messages table query succeeds', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(42, 7));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/hooks',
    });
    await hc.refresh();
    const s = hc.snapshot();
    expect(s.dbRows).toBe(42);
    expect(s.dbStatus).toBe('ok');
    expect(s.unembedded).toBe(7);
  });

  it('sets dbStatus=error when Database constructor throws', async () => {
    const DatabaseCtor = jest.fn(() => { throw new Error('locked'); });
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/hooks',
    });
    await hc.refresh();
    expect(hc.snapshot().dbStatus).toBe('error');
  });

  it('leaves unembedded at 0 when the embedded column is missing', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(10 /* no unembedded arg */));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/hooks',
    });
    await hc.refresh();
    const s = hc.snapshot();
    expect(s.dbRows).toBe(10);
    expect(s.unembedded).toBe(0);
  });

  it('reads vectors from the lance table when getLanceTable returns one', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(1, 0));
    const table = { countRows: async () => 9999 };
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => table as any,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/hooks',
    });
    await hc.refresh();
    expect(hc.snapshot().vectors).toBe(9999);
  });

  it('marks hooksStatus=active when the binary is fresh (< 24h mtime)', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(1, 0));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: {
        existsSync: () => true,
        statSync: () => ({ mtimeMs: Date.now() - 1000 }),
      },
      hookBinaryPath: '/hooks/bin',
    });
    await hc.refresh();
    expect(hc.snapshot().hooksStatus).toBe('active');
  });

  it('marks hooksStatus=stale when the binary is older than 24h', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(1, 0));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: {
        existsSync: () => true,
        statSync: () => ({ mtimeMs: Date.now() - 48 * 3600 * 1000 }),
      },
      hookBinaryPath: '/hooks/bin',
    });
    await hc.refresh();
    expect(hc.snapshot().hooksStatus).toBe('stale');
  });

  it('marks hooksStatus=missing when the binary file does not exist', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(1, 0));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/nope',
    });
    await hc.refresh();
    expect(hc.snapshot().hooksStatus).toBe('missing');
  });

  it('sets ts to the refresh time (monotonic, non-zero)', async () => {
    const DatabaseCtor = jest.fn((_p: string, _o: any) => fakeDb(1, 0));
    const hc = createHealthCache({
      dbPath: '/db', DatabaseCtor: DatabaseCtor as any,
      getLanceTable: () => null,
      fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
      hookBinaryPath: '/x',
    });
    const before = Date.now();
    await hc.refresh();
    const after = Date.now();
    const s: HealthSnapshot = hc.snapshot();
    expect(s.ts).toBeGreaterThanOrEqual(before);
    expect(s.ts).toBeLessThanOrEqual(after);
  });
});
