// @test-type: unit — inferred by chorus-crawl --classify (hermetic) (#4538)
// @domain: search — placed by chorus-crawl's domain rules (#4538)
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import Database from 'better-sqlite3';
import { createHealthCache, HealthSnapshot } from '../src/health-cache';
import { countMessagesInWorker } from '../src/message-counts';

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

/** #4556 — the cache now asks for counts asynchronously; tests keep their fake db behind it. */
function countsFrom(ctor: (p: string, o: unknown) => ReturnType<typeof fakeDb>) {
  return async () => {
    const db = ctor('/db', { readonly: true });
    const dbRows = (db.prepare('SELECT COUNT(*) as cnt FROM messages').get() as { cnt: number }).cnt;
    let unembedded: number | null;
    try { unembedded = (db.prepare('embedded = 0').get() as { cnt: number }).cnt; } catch { unembedded = null; }
    return { dbRows, unembedded };
  };
}

describe('createHealthCache — snapshot', () => {
  it('returns the initial-zero snapshot before any refresh', () => {
    const hc = createHealthCache({
      countMessages: jest.fn(),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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
      countMessages: countsFrom(DatabaseCtor as any),
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

describe('#4556 — the message counts never run on the event loop', () => {
  const base = {
    getLanceTable: () => null,
    fs: { existsSync: () => false, statSync: () => ({ mtimeMs: 0 }) },
    hookBinaryPath: '/hooks',
  };

  // Measured 10-16: a 30 s refresh held the loop 3.97 s in a sync .get. This proves the
  // refresh yields: a timer due in 20 ms fires while a 300 ms count is still running.
  it('a slow count does not stop the loop', async () => {
    let finished = false;
    const hc = createHealthCache({
      ...base,
      countMessages: () => new Promise((r) => setTimeout(() => { finished = true; r({ dbRows: 5, unembedded: 1 }); }, 300)),
    });
    const refreshing = hc.refresh();
    const timerFiredBeforeCount = await new Promise<boolean>((r) => setTimeout(() => r(!finished), 20));
    await refreshing;
    expect(timerFiredBeforeCount).toBe(true);
    expect(hc.snapshot().dbRows).toBe(5);
  });

  // NEGATIVE PROOF: the shape the old code had (a blocking count inside refresh) does stop the loop,
  // so the test above can tell the two apart.
  it('negative proof: a blocking count does stop the loop', async () => {
    const hc = createHealthCache({
      ...base,
      countMessages: () => { const end = Date.now() + 300; while (Date.now() < end) { /* hold the loop */ } return Promise.resolve({ dbRows: 5, unembedded: 1 }); },
    });
    const timer = new Promise<number>((r) => { const t0 = Date.now(); setTimeout(() => r(Date.now() - t0), 20); });
    await hc.refresh();
    expect(await timer).toBeGreaterThanOrEqual(250);
  });

  it('a failed count reads dbStatus error and keeps the last counts', async () => {
    let fail = false;
    const hc = createHealthCache({ ...base, countMessages: () => (fail ? Promise.reject(new Error('locked')) : Promise.resolve({ dbRows: 9, unembedded: 2 })) });
    await hc.refresh();
    fail = true;
    await hc.refresh();
    expect(hc.snapshot().dbStatus).toBe('error');
    expect(hc.snapshot().dbRows).toBe(9);
  });

  it('the worker counts a real sqlite file', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hc-4556-'));
    const file = path.join(dir, 'index.db');
    const db = new Database(file);
    db.exec('CREATE TABLE messages (content TEXT, embedded INTEGER)');
    const ins = db.prepare('INSERT INTO messages VALUES (?, ?)');
    ins.run('x'.repeat(200), 0); ins.run('x'.repeat(200), 1); ins.run('', 0);
    db.close();
    const counts = await countMessagesInWorker(file);
    expect(counts.dbRows).toBe(3);
    expect(counts.unembedded).toBe(1);
    fs.rmSync(dir, { recursive: true, force: true });
  });
});
