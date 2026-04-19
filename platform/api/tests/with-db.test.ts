import { createWithDb } from '../src/with-db';
import { DbNotFoundError } from '../src/server-helpers';

function fakeRes() {
  const self: any = { status_: 200, body_: null, headers: {} };
  self.status = (s: number) => { self.status_ = s; return self; };
  self.json = (b: any) => { self.body_ = b; return self; };
  self.setHeader = (k: string, v: string) => { self.headers[k] = v; };
  return self;
}

function fakeDb() {
  const db: any = { closed: false };
  db.close = () => { db.closed = true; };
  return db;
}

describe('createWithDb', () => {
  it('runs the work function with the opened db and closes it after', async () => {
    const db = fakeDb();
    const openDb = jest.fn(() => db);
    const withDb = createWithDb(openDb as any);
    const res = fakeRes();
    const work = jest.fn(async (d: any) => { expect(d).toBe(db); });
    await withDb(res, work);
    expect(work).toHaveBeenCalledTimes(1);
    expect(db.closed).toBe(true);
  });

  it('responds 503 with the error message when openDb throws DbNotFoundError', async () => {
    const openDb = jest.fn(() => { throw new DbNotFoundError(); });
    const withDb = createWithDb(openDb as any);
    const res = fakeRes();
    const work = jest.fn();
    await withDb(res, work);
    expect(res.status_).toBe(503);
    expect(res.body_).toEqual({ error: 'Chorus index database not found' });
    expect(work).not.toHaveBeenCalled();
  });

  it('rethrows unexpected errors from openDb without calling work', async () => {
    const openDb = jest.fn(() => { throw new Error('different failure'); });
    const withDb = createWithDb(openDb as any);
    const res = fakeRes();
    const work = jest.fn();
    await expect(withDb(res, work)).rejects.toThrow('different failure');
    expect(work).not.toHaveBeenCalled();
  });

  it('still closes the db when work throws', async () => {
    const db = fakeDb();
    const openDb = jest.fn(() => db);
    const withDb = createWithDb(openDb as any);
    const res = fakeRes();
    const work = jest.fn(async () => { throw new Error('work failed'); });
    await expect(withDb(res, work)).rejects.toThrow('work failed');
    expect(db.closed).toBe(true);
  });

  it('awaits async work before closing the db', async () => {
    const db = fakeDb();
    const withDb = createWithDb((() => db) as any);
    const res = fakeRes();
    let resolved = false;
    const work = async (_d: any) => {
      await new Promise(r => setTimeout(r, 5));
      resolved = true;
    };
    await withDb(res, work);
    expect(resolved).toBe(true);
    expect(db.closed).toBe(true);
  });

  it('returns the value produced by work', async () => {
    const withDb = createWithDb((() => fakeDb()) as any);
    const res = fakeRes();
    const result = await withDb(res, async (_d) => 42);
    expect(result).toBe(42);
  });

  it('supports a sync work function', async () => {
    const db = fakeDb();
    const withDb = createWithDb((() => db) as any);
    const res = fakeRes();
    const result = await withDb(res, (_d) => 'sync-value');
    expect(result).toBe('sync-value');
    expect(db.closed).toBe(true);
  });
});
