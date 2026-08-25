// @test-type: unit — pure functions + a fake-timer watch loop; no spine, no fs, no clock
//
// #4004 — reply-gap.ts sat at 28.57% function coverage while its watcher has
// fired reply.delivery.gap 1,983 times in the live spine: the loop deciding
// whether Jeff hears "a role went silent" was the untested part. What Jeff
// sees: an emitted-but-never-rendered reply alarms ONCE, a rendered reply
// never alarms, and neither a corrupt spine line nor an unreadable spine can
// take the watcher down quietly.
import {
  detectGaps,
  gapKey,
  parseSpineTail,
  startReplyGapWatch,
  REPLY_GAP_WINDOW_MS,
  type Gap,
} from './reply-gap';

const T0 = Date.parse('2026-08-25T12:00:00.000Z');
const emitted = (role: string, hash: string, atMs: number) =>
  JSON.stringify({ event: 'reply.emitted', role, hash, timestamp: new Date(atMs).toISOString() });
const rendered = (role: string, hash: string) =>
  JSON.stringify({ event: 'reply.rendered', role, hash, timestamp: new Date(T0).toISOString() });

describe('#4004 reply-gap watch loop', () => {
  afterEach(() => { jest.useRealTimers(); });

  it('raises a gap for a reply emitted past the window and never rendered', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () => emitted('silas', 'abc', T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(1000);
    stop();
    expect(gaps.map(gapKey)).toEqual(['silas:abc']);
  });

  it('fires ONCE — a still-unrendered reply does not re-alarm every scan', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () => emitted('wren', 'zz', T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(3000);
    stop();
    expect(gaps).toHaveLength(1);
  });

  it('NEGATIVE PROOF: a rendered reply raises nothing — the check can tell the states apart', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () =>
      [emitted('kade', 'q1', T0 - REPLY_GAP_WINDOW_MS - 1), rendered('kade', 'q1')].join('\n');
    const stop = startReplyGapWatch(tail, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(gaps).toHaveLength(0);
  });

  it('a corrupt spine line is skipped, and the watcher keeps reporting real gaps', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () =>
      ['{not json', '', emitted('silas', 'ok1', T0 - REPLY_GAP_WINDOW_MS - 1)].join('\n');
    const stop = startReplyGapWatch(tail, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(1000);
    stop();
    expect(gaps.map((g) => g.role)).toEqual(['silas']);
  });

  it('a failing tail read is LOUD but not fatal — the loop survives to scan again', async () => {
    jest.useFakeTimers();
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    const gaps: Gap[] = [];
    let call = 0;
    const tail = async () => {
      call += 1;
      if (call === 1) throw new Error('spine unreadable');
      return emitted('silas', 'after', T0 - REPLY_GAP_WINDOW_MS - 1);
    };
    const stop = startReplyGapWatch(tail, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(err).toHaveBeenCalled();
    expect(gaps.map((g) => g.hash)).toEqual(['after']);
    err.mockRestore();
  });

  it('stop() ends the watch — no scans after teardown', async () => {
    jest.useFakeTimers();
    let reads = 0;
    const stop = startReplyGapWatch(async () => { reads += 1; return ''; }, async () => {}, 1000);
    await jest.advanceTimersByTimeAsync(1000);
    const afterFirst = reads;
    stop();
    await jest.advanceTimersByTimeAsync(5000);
    expect(reads).toBe(afterFirst);
  });

  it('detectGaps ignores an unparseable timestamp rather than throwing', () => {
    const evs = parseSpineTail(
      JSON.stringify({ event: 'reply.emitted', role: 'silas', hash: 'h', timestamp: 'not-a-date' }),
    );
    expect(detectGaps(evs, T0, REPLY_GAP_WINDOW_MS, new Set())).toEqual([]);
  });
});
