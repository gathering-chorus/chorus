/**
 * Attribution tests — #2048
 *
 * Prior work: session-tailer (#1665) bridges terminal sessions to Clearing.
 * #1706 filtered role-to-role noise. #2035 preserved Jeff's words.
 * Bug: ALL user-type messages attributed as from:'jeff' (line 207-212).
 * Nudges inject into terminals as user input — so role nudges show as Jeff's.
 * Fix: detect [nudge from <role>] pattern, attribute to sending role.
 */

import { MessageRouter } from '../src/router';

describe('#2048: Message attribution', () => {
  let router: MessageRouter;

  beforeEach(() => {
    router = new MessageRouter();
  });

  test('nudge messages attributed to sending role, not jeff', () => {
    router.ingest({
      from: 'kade',
      text: '[demo] #2036 — Clearing terminal bridge fixed.',
      ts: new Date().toISOString(),
      type: 'role-response',
    });
    const msgs = router.getRecent(10, true);
    const last = msgs[msgs.length - 1];
    expect(last.from).toBe('kade');
    expect(last.from).not.toBe('jeff');
  });

  test('jeff typed messages stay attributed to jeff', () => {
    router.ingest({
      from: 'jeff',
      text: 'test',
      ts: new Date().toISOString(),
      type: 'jeff-input',
    });
    const msgs = router.getRecent(10);
    const last = msgs[msgs.length - 1];
    expect(last.from).toBe('jeff');
    expect(last.type).toBe('jeff-input');
  });

  test('role-response messages are visible in Clearing', () => {
    router.ingest({
      from: 'silas',
      text: 'gate:arch PASS on #2015',
      ts: new Date().toISOString(),
      type: 'role-response',
    });
    const msgs = router.getRecent(10);
    expect(msgs.length).toBe(1);
    expect(msgs[0].visible).toBe(true);
  });
});
