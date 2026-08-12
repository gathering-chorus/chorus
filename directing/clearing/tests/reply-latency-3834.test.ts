// @test-type: unit — feeds transcript lines to the tailer with fake timers; no fs.watch, no sessions on disk, no network.
/**
 * #3834 — a short answer should appear in seconds, not 45 of them.
 *
 * Measured 2026-08-12: Jeff asked "anyone there?" at 16:08:04. The reply was
 * written at 16:08:13 — nine seconds — and he saw it three quarters of a minute
 * later. His words: "it makes it impossible for any of u to give a simple
 * response", and the sharper one, "u have to burn at least 45 seconds of tokens
 * to say yes" — literally true, since the only way to make a short answer
 * appear was to stay silent for 45 seconds afterwards.
 *
 * Cause: promotion was decided by a stopwatch. A clock cannot tell "finished"
 * from "still going". The transcript can — stop_reason 'end_turn' means the
 * model stopped without reaching for a tool.
 *
 * The trap these tests are built around: making it FAST is easy and making it
 * fast WITHOUT unfolding every mid-turn status note is the actual job. A room
 * that shows everything is not better than a room that shows things late.
 */

import { SessionTailer } from '../src/session-tailer';
import { MessageRouter } from '../src/router';

/** A transcript line as the session JSONL writes it. */
const assistantLine = (text: string, stopReason: string) => JSON.stringify({
  type: 'assistant',
  timestamp: '2026-08-12T16:08:13.000Z',
  message: { stop_reason: stopReason, content: [{ type: 'text', text }] },
});

const jeffLine = (text: string) => JSON.stringify({
  type: 'user',
  timestamp: '2026-08-12T16:08:04.000Z',
  message: { content: [{ type: 'text', text }] },
});

/** Drive the tailer's line handling directly — no files, no watchers. */
function feed(tailer: SessionTailer, role: string, line: string): void {
  (tailer as unknown as { processLine: (r: string, l: string) => void }).processLine(role, line);
}

function visible(router: MessageRouter): Array<{ from: string; text: string; type: string }> {
  return router.getRecent(50).map((m) => ({ from: m.from, text: m.text, type: m.type }));
}

describe('#3834 a finished reply appears at once', () => {
  beforeEach(() => { jest.useFakeTimers(); });
  afterEach(() => { jest.useRealTimers(); });

  test('a reply whose turn ENDED is visible without waiting out any clock', () => {
    const router = new MessageRouter();
    const tailer = new SessionTailer(router);
    feed(tailer, 'wren', jeffLine('anyone there?'));
    feed(tailer, 'wren', assistantLine('Here.', 'end_turn'));

    // Not a single timer advanced. This is the whole card: before the fix the
    // room showed nothing until 45s of silence had passed.
    jest.advanceTimersByTime(0);
    expect(visible(router).some((m) => m.from === 'wren' && m.text === 'Here.' && m.type === 'role-response')).toBe(true);
  });

  test('NEGATIVE PROOF: a MID-TURN note does not get promoted', () => {
    // The failure that would make this fast and useless. A note emitted while
    // the model is still working (stop_reason tool_use) must stay folded — if
    // it promoted, every "checking the logs…" line would land in the room as
    // though it were an answer.
    const router = new MessageRouter();
    const tailer = new SessionTailer(router);
    feed(tailer, 'wren', jeffLine('anyone there?'));
    feed(tailer, 'wren', assistantLine('Checking the logs…', 'tool_use'));

    jest.advanceTimersByTime(3500); // past the coalescing debounce
    const shown = visible(router);
    expect(shown.some((m) => m.text === 'Checking the logs…' && m.type === 'role-response')).toBe(false);
  });

  test('a mid-turn note folds and the FINAL reply is the one that shows', () => {
    const router = new MessageRouter();
    const tailer = new SessionTailer(router);
    feed(tailer, 'wren', jeffLine('what happened to the nightly?'));
    feed(tailer, 'wren', assistantLine('Looking…', 'tool_use'));
    jest.advanceTimersByTime(3500);
    feed(tailer, 'wren', assistantLine('It was the open handle. Fixed.', 'end_turn'));
    jest.advanceTimersByTime(0);

    const responses = visible(router).filter((m) => m.type === 'role-response');
    expect(responses).toHaveLength(1);
    expect(responses[0].text).toBe('It was the open handle. Fixed.');
  });

  test('NEGATIVE PROOF: without a turn-end marker it still promotes, but only on the backstop', () => {
    // Older transcripts, a crash mid-write, a truncated line: the marker may
    // never arrive. The reply must not be lost — but it must NOT appear
    // instantly either, or the mid-turn fold above would be defeated for every
    // message lacking the field.
    const router = new MessageRouter();
    const tailer = new SessionTailer(router);
    feed(tailer, 'wren', jeffLine('you there?'));
    feed(tailer, 'wren', assistantLine('Here.', 'no_such_reason'));

    jest.advanceTimersByTime(3500);
    expect(visible(router).some((m) => m.type === 'role-response')).toBe(false);

    jest.advanceTimersByTime(9000); // past the 8s backstop
    expect(visible(router).some((m) => m.text === 'Here.' && m.type === 'role-response')).toBe(true);
  });

  test('NEGATIVE PROOF: the backstop is not 45 seconds any more', () => {
    // The old value, pinned as gone. If someone restores 45000, this fails —
    // which matters because nothing else would: the room would simply feel
    // dead again and we would rediscover it by Jeff waiting.
    const router = new MessageRouter();
    const tailer = new SessionTailer(router);
    feed(tailer, 'wren', jeffLine('ping'));
    feed(tailer, 'wren', assistantLine('pong', 'unknown'));
    jest.advanceTimersByTime(3500 + 10000);
    expect(visible(router).some((m) => m.text === 'pong' && m.type === 'role-response')).toBe(true);
  });
});
