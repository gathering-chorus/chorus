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

describe('#2049: Skill output filtering', () => {
  test('structured skill output is hidden from Clearing', () => {
    const router = new MessageRouter();
    const skillOutputs = [
      'Auto-checked 3 AC items on #2017',
      'Demo started: #2015 — Structured logging',
      'Done: #2048',
      'Moved #2049 to WIP',
      'Accepted #2036 — committed and pushed.',
      'INJECT_FAILED for silas at 2026-04-14 08:44',
    ];
    for (const text of skillOutputs) {
      router.ingest({ from: 'kade', text, ts: new Date().toISOString(), type: 'pm-thinking' });
    }
    const visible = router.getRecent(20);
    for (const msg of visible) {
      expect(msg.text).not.toMatch(/^(Auto-checked|Demo started:|Done:|Moved #|Accepted #\d+|INJECT_FAILED)/);
    }
  });

  test('role thinking/commentary passes through', () => {
    const router = new MessageRouter();
    router.ingest({
      from: 'kade',
      text: 'The fix is two small changes in session-tailer.ts',
      ts: new Date().toISOString(),
      type: 'pm-thinking',
    });
    const msgs = router.getRecent(10);
    expect(msgs.length).toBe(1);
    expect(msgs[0].visible).toBe(true);
  });
});
