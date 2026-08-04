// @test-type: unit — constructed res/db fixtures, no live services
// #3750 AC2 — one slow worker must cost one request, never the process.
// 2026-08-04 10:47: an fts pool-timeout rejection escaped withDb → Express 4
// async void → unhandledRejection → FATAL exit(1). NEGATIVE PROOF (#3734):
// the fixture below drives that exact rejection shape and asserts it settles
// as a typed 503 response with NO rethrow (the rethrow IS the crash).
import { createWithDb } from '../src/with-db';
import { DbNotFoundError } from '../src/server-helpers';

function makeRes() {
  const calls: Array<{ status: number; body: unknown }> = [];
  let lastStatus = 0;
  const res = {
    headersSent: false,
    status(s: number) { lastStatus = s; return res; },
    json(b: unknown) { calls.push({ status: lastStatus, body: b }); res.headersSent = true; return res; },
  };
  return { res, calls };
}

const db = { close: jest.fn() };

describe('#3750 withDb — per-request degradation, never a rethrow', () => {
  afterEach(() => jest.restoreAllMocks());

  it('pool timeout rejection → typed 503, promise RESOLVES (the crash shape refused)', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {});
    const withDb = createWithDb(() => db);
    const { res, calls } = makeRes();
    await expect(
      withDb(res, async () => { throw new Error('fts worker timeout after 10000ms'); }),
    ).resolves.toBeUndefined(); // resolves — a reject here is the process-killer
    expect(calls).toEqual([{ status: 503, body: { error: 'fts worker timeout after 10000ms', degraded: true } }]);
    expect(db.close).toHaveBeenCalled();
  });

  it('non-timeout failure → typed 500, still no rethrow', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {});
    const withDb = createWithDb(() => db);
    const { res, calls } = makeRes();
    await expect(withDb(res, async () => { throw new Error('boom'); })).resolves.toBeUndefined();
    expect(calls[0].status).toBe(500);
  });

  it('no double-response when headers already sent', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {});
    const withDb = createWithDb(() => db);
    const { res, calls } = makeRes();
    await withDb(res, async () => {
      (res as { json: (b: unknown) => unknown }).json({ ok: true }); // route responded, then failed
      throw new Error('late failure');
    });
    expect(calls).toHaveLength(1); // only the route's own response
  });

  it('success passthrough and DbNotFound → 503 are unchanged', async () => {
    const withDb = createWithDb(() => db);
    const { res: r1 } = makeRes();
    await expect(withDb(r1, async () => 42)).resolves.toBe(42);
    const withDbMissing = createWithDb(() => { throw new DbNotFoundError('index.db missing'); });
    const { res: r2, calls: c2 } = makeRes();
    await expect(withDbMissing(r2, async () => 42)).resolves.toBeUndefined();
    expect(c2[0].status).toBe(503);
  });
});
