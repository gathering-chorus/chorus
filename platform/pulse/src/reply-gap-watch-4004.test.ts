// @card: #4004
// @owner: wren
/* eslint-disable @typescript-eslint/require-await -- injected async doubles document the Promise contract the watcher awaits */
// @test-type: unit — pure functions + a fake-timer watch loop; no spine, no fs, no clock
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
//
// #4004 — reply-gap.ts sat at 28.57% function coverage while its watcher has
// fired reply.delivery.gap 1,983 times in the live spine: the loop deciding
// whether Jeff hears "a role went silent" was the untested part. What Jeff
// sees: an emitted reply missing from the room alarms ONCE, a rendered reply
// never alarms (#4887: the room's own rows decide), and neither a corrupt spine line nor an unreadable spine can
// take the watcher down quietly.
import {
  detectGaps,
  gapKey,
  parseSpineTail,
  startReplyGapWatch,
  REPLY_GAP_WINDOW_MS,
  type Gap,
  type RoomMsg,
} from './reply-gap';
import { contentHash } from '../../chorus-sdk/reply/hash';

const T0 = Date.parse('2026-08-25T12:00:00.000Z');
const emitted = (role: string, hash: string, atMs: number) =>
  JSON.stringify({ event: 'reply.emitted', role, hash, timestamp: new Date(atMs).toISOString() });
// The room's record, reaching back before every emitted reply in these tests.
const roomWith = (...texts: string[]) => async (): Promise<RoomMsg[]> => [
  { from: 'wren', text: 'an older row', ts: new Date(T0 - 3_600_000).toISOString() },
  ...texts.map((text) => ({ from: 'kade', text, ts: new Date(T0).toISOString() })),
];
const room = roomWith();

describe('#4004 reply-gap watch loop', () => {
  afterEach(() => { jest.useRealTimers(); });

  it('raises a gap for a reply emitted past the window and never rendered', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () => emitted('silas', 'abc', T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, room, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(1000);
    stop();
    expect(gaps.map(gapKey)).toEqual(['silas:abc']);
  });

  it('fires ONCE — a still-unrendered reply does not re-alarm every scan', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () => emitted('wren', 'zz', T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, room, async (g) => { gaps.push(g); }, 1000,
      REPLY_GAP_WINDOW_MS, () => T0);
    await jest.advanceTimersByTimeAsync(3000);
    stop();
    expect(gaps).toHaveLength(1);
  });

  it('NEGATIVE PROOF: a reply in the room raises nothing — the check can tell the states apart', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const tail = async () => emitted('kade', contentHash('q1'), T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, roomWith('q1'), async (g) => { gaps.push(g); }, 1000,
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
    const stop = startReplyGapWatch(tail, room, async (g) => { gaps.push(g); }, 1000,
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
    const stop = startReplyGapWatch(tail, room, async (g) => { gaps.push(g); }, 1000,
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
    const stop = startReplyGapWatch(async () => { reads += 1; return ''; }, room, async () => {}, 1000);
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
    expect(detectGaps(evs, [], T0, REPLY_GAP_WINDOW_MS, new Set())).toEqual({ gaps: [], unmeasured: 0 });
  });

  it('an unreadable room raises no gap; going down and coming back each report once', async () => {
    jest.useFakeTimers();
    const gaps: Gap[] = [];
    const states: string[] = [];
    let up = false;
    const tail = async () => emitted('silas', 'lost?', T0 - REPLY_GAP_WINDOW_MS - 1);
    const stop = startReplyGapWatch(tail, async () => (up ? roomWith('x')() : null), async (g) => { gaps.push(g); },
      1000, REPLY_GAP_WINDOW_MS, () => T0, async (s) => { states.push(`${s.room}:${s.unmeasured}`); });
    await jest.advanceTimersByTimeAsync(3000);
    expect(gaps).toHaveLength(0);
    expect(states).toEqual(['unreadable:1']);
    up = true;
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(states).toEqual(['unreadable:1', 'readable:0']);
  });
});
