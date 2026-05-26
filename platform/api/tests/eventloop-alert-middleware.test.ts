// #3089 to #3096 unit tests — request-op middleware: ALS-bound op, async-resume-safe.
// The op set by the middleware lives in an AsyncLocalStorage run-block, NOT in a
// module-level slot. Two consequences these tests pin down:
//   1. The op is only observable INSIDE next() (and the async tree it spawns).
//      Outside the run block the reader falls back to the scheduled-job slot.
//   2. Two parallel requests each see THEIR OWN op — no clobber, no last-write-wins.
import { EventEmitter } from 'events';
import { makeRequestOpMiddleware, getCurrentOp, setCurrentOp, type ResLike } from '../src/eventloop-alert';

function fakeReq(method: string, path: string): { method: string; path: string } {
  return { method, path };
}

describe('makeRequestOpMiddleware (#3089 to #3096)', () => {
  beforeEach(() => setCurrentOp(null));

  it('exposes op as METHOD PATH while next is running', () => {
    const mw = makeRequestOpMiddleware();
    const res = new EventEmitter() as unknown as ResLike;
    let inside = 'NOT-SET';
    let nextCalled = false;
    mw(fakeReq('GET', '/api/chorus/search'), res, () => {
      nextCalled = true;
      inside = getCurrentOp();
    });
    expect(inside).toBe('GET /api/chorus/search');
    expect(nextCalled).toBe(true);
  });

  it('op is scoped to the request context — outside the run block, the reader falls back', () => {
    const mw = makeRequestOpMiddleware();
    const res = new EventEmitter() as unknown as ResLike;
    mw(fakeReq('POST', '/api/chorus/cards'), res, () => { /* sync next */ });
    // We're now outside the ALS run; no scheduled-job slot set → 'unknown'.
    expect(getCurrentOp()).toBe('unknown');
  });

  it('does not leak the op from one request into another (no shared module slot)', () => {
    const mw = makeRequestOpMiddleware();
    let firstSeen = 'NOT-SET';
    let secondSeen = 'NOT-SET';
    mw(fakeReq('GET', '/a'), new EventEmitter() as unknown as ResLike, () => {
      firstSeen = getCurrentOp();
    });
    // Outside the first run, before the second: should be unknown, not 'GET /a'.
    expect(getCurrentOp()).toBe('unknown');
    mw(fakeReq('POST', '/b'), new EventEmitter() as unknown as ResLike, () => {
      secondSeen = getCurrentOp();
    });
    expect(firstSeen).toBe('GET /a');
    expect(secondSeen).toBe('POST /b');
    expect(getCurrentOp()).toBe('unknown');
  });

  it('Class A (closed): two parallel requests each see their own op — no single-slot clobber', async () => {
    const mw = makeRequestOpMiddleware();
    let aOpAfterResume = 'NOT-SET';
    let bOpAfterResume = 'NOT-SET';

    const aDone = new Promise<void>((resolve) => {
      mw(fakeReq('GET', '/first'), new EventEmitter() as unknown as ResLike, async () => {
        await new Promise((r) => setImmediate(r));
        aOpAfterResume = getCurrentOp();
        resolve();
      });
    });

    const bDone = new Promise<void>((resolve) => {
      mw(fakeReq('GET', '/second'), new EventEmitter() as unknown as ResLike, async () => {
        await new Promise((r) => setImmediate(r));
        bOpAfterResume = getCurrentOp();
        resolve();
      });
    });

    await Promise.all([aDone, bDone]);
    expect(aOpAfterResume).toBe('GET /first');
    expect(bOpAfterResume).toBe('GET /second');
  });

  it('coexists with the scheduled-job slot: ALS op wins during a request, slot is restored after', () => {
    const mw = makeRequestOpMiddleware();
    setCurrentOp('boardCache');
    let insideRequest = 'NOT-SET';
    mw(fakeReq('GET', '/api/chorus/search'), new EventEmitter() as unknown as ResLike, () => {
      insideRequest = getCurrentOp();
    });
    expect(insideRequest).toBe('GET /api/chorus/search');
    // After the request, the cron's slot is untouched — no `finish`/`close` clear stomped on it.
    expect(getCurrentOp()).toBe('boardCache');
  });
});
