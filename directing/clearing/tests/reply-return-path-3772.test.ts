// @test-type: unit — router stub, fixture JSONL lines, env-tuned timers; no live sessions, brings its own world.
/**
 * #3772 — the Clearing return path.
 *
 * Jeff, 2026-08-06: "does anyone get my messages?" — his room messages reached
 * every role session, but role replies surfaced only as debounced pm-thinking,
 * which the #3747 tree folds into "+n steps". The room looked dead while three
 * sessions answered. These tests describe what Jeff sees:
 *   - his message → the role's REPLY appears as a visible role-response
 *   - mid-turn status notes stay folded (pm-thinking) — "i want the pm thinking
 *     just folded"
 *   - unprompted assistant text keeps the old folded behavior
 *   - NEGATIVE PROOF: the silent-room shape (activity, no reply) is DETECTED
 *     by findSilentReplies on a pre-#3772-style transcript.
 */
process.env.CLEARING_REPLY_QUIET_MS = '4000'; // must exceed the 3s flush debounce, else the candidate promotes between flushes

import { SessionTailer, findSilentReplies, RoomRecord } from '../src/session-tailer';

interface Ingested { from: string; text: string; ts: string; type: string }

function makeRouterStub() {
  const got: Ingested[] = [];
  return { got, router: { ingest: (m: Ingested) => { got.push(m); } } };
}

function userLine(text: string): string {
  return JSON.stringify({ type: 'user', timestamp: '2026-08-06T15:00:00Z', message: { content: text } });
}
function assistantLine(text: string, ts = '2026-08-06T15:00:10Z'): string {
  return JSON.stringify({ type: 'assistant', timestamp: ts, message: { content: [{ type: 'text', text }] } });
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe('#3772 return path — reply classification', () => {
  test('jeff-input then one assistant text → visible role-response after quiet', async () => {
    const { got, router } = makeRouterStub();
    const tailer = new SessionTailer(router as never);
    const feed = (line: string) => (tailer as never as { processLine(r: string, l: string): void }).processLine('wren', line);

    feed(userLine('can i see the security domain?'));
    feed(assistantLine('Yes — here is the link, 25 APISurfaces render now.'));

    await sleep(3200);  // flush debounce
    await sleep(4200);  // quiet window (4s in tests)

    const types = got.map((m) => `${m.type}:${m.from}`);
    expect(types).toContain('jeff-input:jeff');
    expect(types).toContain('role-response:wren');
    const reply = got.find((m) => m.type === 'role-response');
    expect(reply!.text).toMatch(/25 APISurfaces/);
    expect(got.filter((m) => m.type === 'pm-thinking')).toHaveLength(0);
  }, 15000);

  test('mid-turn status folds; LAST text is the reply', async () => {
    const { got, router } = makeRouterStub();
    const tailer = new SessionTailer(router as never);
    const feed = (line: string) => (tailer as never as { processLine(r: string, l: string): void }).processLine('wren', line);

    feed(userLine('status?'));
    feed(assistantLine('Checking the pipeline now.', '2026-08-06T15:00:05Z'));
    await sleep(3200); // first flush → becomes candidate
    feed(assistantLine('Pipeline is green — landed as 82d84c00a.', '2026-08-06T15:01:00Z'));
    await sleep(3200); // second flush → demotes first to pm-thinking, becomes candidate
    await sleep(4200); // quiet → promote

    const folded = got.filter((m) => m.type === 'pm-thinking');
    const replies = got.filter((m) => m.type === 'role-response');
    expect(folded).toHaveLength(1);
    expect(folded[0].text).toMatch(/Checking the pipeline/);
    expect(replies).toHaveLength(1);
    expect(replies[0].text).toMatch(/landed as 82d84c00a/);
  }, 20000);

  test('jeff typing again promotes the pending reply BEFORE his new input', async () => {
    const { got, router } = makeRouterStub();
    const tailer = new SessionTailer(router as never);
    const feed = (line: string) => (tailer as never as { processLine(r: string, l: string): void }).processLine('wren', line);

    feed(userLine('first question'));
    feed(assistantLine('First answer.'));
    await sleep(3200); // flush → candidate (quiet not yet elapsed matters little at 100ms, so feed fast)
    feed(userLine('second question'));

    const seq = got.map((m) => m.type);
    const replyIdx = seq.indexOf('role-response');
    const secondInputIdx = seq.lastIndexOf('jeff-input');
    expect(replyIdx).toBeGreaterThan(-1);
    expect(replyIdx).toBeLessThan(secondInputIdx);
  }, 15000);

  test('unprompted assistant text stays folded (no jeff-input pending)', async () => {
    const { got, router } = makeRouterStub();
    const tailer = new SessionTailer(router as never);
    const feed = (line: string) => (tailer as never as { processLine(r: string, l: string): void }).processLine('wren', line);

    feed(assistantLine('Background observation about the board.'));
    await sleep(3500);

    expect(got.filter((m) => m.type === 'role-response')).toHaveLength(0);
    expect(got.filter((m) => m.type === 'pm-thinking')).toHaveLength(1);
  });
});

describe('#3772 NEGATIVE PROOF — silent-room state is detectable', () => {
  test('pre-#3772 transcript shape (activity, no reply) goes RED', () => {
    const oldStyle: RoomRecord[] = [
      { from: 'jeff', text: 'does anyone get my messages?', ts: 't1', type: 'jeff-input' },
      { from: 'wren', text: 'long folded thinking', ts: 't2', type: 'pm-thinking' },
      { from: 'jeff', text: 'apparently no…', ts: 't3', type: 'jeff-input' },
      { from: 'kade', text: 'more folded thinking', ts: 't4', type: 'pm-thinking' },
    ];
    const violations = findSilentReplies(oldStyle);
    expect(violations).toHaveLength(2);
    expect(violations[0]).toEqual({ promptTs: 't1', role: 'wren' });
    expect(violations[1]).toEqual({ promptTs: 't3', role: 'kade' });
  });

  test('post-#3772 transcript shape (reply present) is green', () => {
    const fixed: RoomRecord[] = [
      { from: 'jeff', text: 'can i see it?', ts: 't1', type: 'jeff-input' },
      { from: 'wren', text: 'folded note', ts: 't2', type: 'pm-thinking' },
      { from: 'wren', text: 'Yes — link here.', ts: 't3', type: 'role-response' },
    ];
    expect(findSilentReplies(fixed)).toHaveLength(0);
  });

  test('no assistant activity at all is NOT a violation (role may be offline — different failure, different alarm)', () => {
    const quiet: RoomRecord[] = [
      { from: 'jeff', text: 'anyone?', ts: 't1', type: 'jeff-input' },
    ];
    expect(findSilentReplies(quiet)).toHaveLength(0);
  });
});
