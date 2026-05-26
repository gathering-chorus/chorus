// #3050 — the eventloop alert must report ONLY what it measured (duration + time),
// and must NOT fabricate a causal story. This test is the honest-message contract:
// the day's whole lesson — an alert that asserts the unobservable can't be trusted.
// #3079 — added op param; op=unknown → access-log pointer; op=name → captured-op note.
// #3096 — ALS-bound request op survives async resumption; scheduled-job slot still works.
import { formatBlockAlert, makeRequestOpMiddleware, setCurrentOp, getCurrentOp } from '../src/eventloop-alert';

const TS = '2026-05-23T20:00:00.000Z';

describe('#3050 formatBlockAlert — honest, no fabrication', () => {
  it('carries the real measured duration', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    expect(a.duration_ms).toBe(1234);
    expect(a.message).toContain('1234');
  });

  it('carries the timestamp so the slow request is correlatable in the access log (op=unknown)', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    // Storage contract: `ts` field stays ISO/UTC (spine event correlation,
    // db rows, cross-machine ordering). Render contract: the message body
    // is what humans read, and it renders Boston (#3093 — render-vs-storage
    // boundary, Jeff doesn't read UTC). Two separate concerns, two separate
    // assertions, one helper (boston()) responsible for the render side.
    expect(a.ts).toBe(TS);
    // 2026-05-23T20:00:00.000Z → 16:00:00 EDT (UTC-4 in May).
    expect(a.message).toContain('2026-05-23 16:00:00 EDT');
    expect(a.message).not.toContain(TS); // no raw ISO in human surface
    expect(a.message.toLowerCase()).toContain('access log');
  });

  it('reports captured op when op is known', () => {
    const a = formatBlockAlert(1234, TS, 'scheduledReindex');
    expect(a.op).toBe('scheduledReindex');
    expect(a.message.toLowerCase()).toContain('captured op: scheduledreindex');
    expect(a.message.toLowerCase()).not.toContain('access log');
  });

  it('does NOT fabricate a causal story it cannot observe', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    const m = a.message.toLowerCase();
    expect(m).not.toContain('blocked every role');
    expect(m).not.toContain('stalled');
    expect(m).not.toContain('sync git');
    expect(m).not.toContain('#3039 freeze class');
  });
});

describe('#3096 ALS-bound request op survives async resumption', () => {
  const makeRes = (): { once: jest.Mock } => ({ once: jest.fn() });

  afterEach(() => setCurrentOp(null));

  it('returns the middleware-set op inside the handler context', async () => {
    const mw = makeRequestOpMiddleware();
    let captured = 'NOT-SET';
    await new Promise<void>((resolve) => {
      mw({ method: 'GET', path: '/api/chorus/search' }, makeRes(), () => {
        captured = getCurrentOp();
        resolve();
      });
    });
    expect(captured).toBe('GET /api/chorus/search');
  });

  it('Class A: slow A awaits, fast B enters+exits, A resumes — A still reads its own op (not B, not unknown)', async () => {
    const mw = makeRequestOpMiddleware();
    let aOpAfterResume = 'NOT-SET';

    // Slow request A: enters, awaits a microtask, resumes, reads its op.
    const aDone = new Promise<void>((resolve) => {
      mw({ method: 'GET', path: '/api/chorus/search' }, makeRes(), async () => {
        await new Promise((r) => setImmediate(r));
        aOpAfterResume = getCurrentOp();
        resolve();
      });
    });

    // Fast request B: runs to completion while A is awaiting. With the old
    // single-slot middleware, B would clobber A's op to 'GET /freshness'
    // then clear it to null; A would resume reading 'unknown'.
    await new Promise<void>((resolve) => {
      mw({ method: 'GET', path: '/freshness' }, makeRes(), () => resolve());
    });

    await aDone;
    expect(aOpAfterResume).toBe('GET /api/chorus/search');
  });

  it('outside any request context falls back to the scheduled-job slot, then unknown', () => {
    expect(getCurrentOp()).toBe('unknown');
    setCurrentOp('boardCache');
    expect(getCurrentOp()).toBe('boardCache');
    setCurrentOp(null);
    expect(getCurrentOp()).toBe('unknown');
  });

  it('ALS op wins over the scheduled-job slot when both are set (request running over a sticky cron)', async () => {
    setCurrentOp('boardCache');
    const mw = makeRequestOpMiddleware();
    let inside = 'NOT-SET';
    await new Promise<void>((resolve) => {
      mw({ method: 'GET', path: '/api/chorus/search' }, makeRes(), () => {
        inside = getCurrentOp();
        resolve();
      });
    });
    expect(inside).toBe('GET /api/chorus/search');
    // After the request, the slot is intact for the scheduled job.
    expect(getCurrentOp()).toBe('boardCache');
  });
});
