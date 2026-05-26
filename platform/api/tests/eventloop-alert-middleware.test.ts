// #3089 unit tests — request-op middleware (kill op=unknown for request handlers).
import { EventEmitter } from 'events';
import { makeRequestOpMiddleware, getCurrentOp, setCurrentOp, type ResLike } from '../src/eventloop-alert';

function fakeReq(method: string, path: string): { method: string; path: string } {
  return { method, path };
}

describe('makeRequestOpMiddleware (#3089)', () => {
  beforeEach(() => setCurrentOp(null));

  it('sets op = `${method} ${path}` on request entry, calls next()', () => {
    const mw = makeRequestOpMiddleware();
    const res = new EventEmitter() as unknown as ResLike;
    let nextCalled = false;
    mw(fakeReq('GET', '/api/chorus/search'), res, () => { nextCalled = true; });
    expect(getCurrentOp()).toBe('GET /api/chorus/search');
    expect(nextCalled).toBe(true);
  });

  it('clears op when the response emits `finish`', () => {
    const mw = makeRequestOpMiddleware();
    const res = new EventEmitter();
    mw(fakeReq('POST', '/api/chorus/cards'), res as unknown as ResLike, () => { /* noop */ });
    expect(getCurrentOp()).toBe('POST /api/chorus/cards');
    res.emit('finish');
    expect(getCurrentOp()).toBe('unknown');
  });

  it('clears op when the response emits `close` (client disconnect)', () => {
    const mw = makeRequestOpMiddleware();
    const res = new EventEmitter();
    mw(fakeReq('GET', '/api/chorus/crawl/photos'), res as unknown as ResLike, () => { /* noop */ });
    res.emit('close');
    expect(getCurrentOp()).toBe('unknown');
  });

  it("doesn't leak the op from a finished request into the next one", () => {
    const mw = makeRequestOpMiddleware();
    const r1 = new EventEmitter();
    mw(fakeReq('GET', '/a'), r1 as unknown as ResLike, () => { /* noop */ });
    r1.emit('finish');
    expect(getCurrentOp()).toBe('unknown');
    const r2 = new EventEmitter();
    mw(fakeReq('POST', '/b'), r2 as unknown as ResLike, () => { /* noop */ });
    expect(getCurrentOp()).toBe('POST /b');
  });

  it('preserves the most-recent request when a second arrives mid-flight (single-slot, documented)', () => {
    // sync handlers serialize on the event loop, so only one is on-loop at a time;
    // for the common loop-blocker case, single-slot is correct. The async-resume edge
    // is the known limitation noted in the helper's doc comment.
    const mw = makeRequestOpMiddleware();
    const r1 = new EventEmitter();
    mw(fakeReq('GET', '/first'), r1 as unknown as ResLike, () => { /* noop */ });
    expect(getCurrentOp()).toBe('GET /first');
    const r2 = new EventEmitter();
    mw(fakeReq('GET', '/second'), r2 as unknown as ResLike, () => { /* noop */ });
    expect(getCurrentOp()).toBe('GET /second');
  });
});
