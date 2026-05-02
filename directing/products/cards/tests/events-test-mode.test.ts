/**
 * NODE_ENV=test spine-emission suppression regression (#2241).
 *
 * Context: wave 2 of #2241 leaked test-card spine events into the real
 * platform/logs/chorus.log, which the Chorus index then surfaced as fake
 * 'Accepted' bubbles in The Clearing (Jeff flagged 2026-04-19 11:27, Wren
 * to Kade). The guard in events.ts short-circuits emit() when NODE_ENV is
 * 'test' so gates catch this next time.
 *
 * This test proves the guard: calling emitSpineEvent under the test env
 * adds zero lines to the chorus.log file.
 */

import * as fs from 'fs';
import * as path from 'path';
import { emitSpineEvent } from '../src/events';

describe('events.ts — NODE_ENV=test suppresses spine emissions', () => {
  const logPath = path.resolve(__dirname, '..', '..', '..', 'platform', 'logs', 'chorus.log');

  it('emitSpineEvent does not append to chorus.log under jest', () => {
    // Jest sets NODE_ENV=test automatically.
    expect(process.env.NODE_ENV).toBe('test');
    const before = fs.existsSync(logPath) ? fs.statSync(logPath).size : 0;
    emitSpineEvent('card.item.created', 'kade', { card_id: '9999999', title: 'regression fixture' });
    emitSpineEvent('card.accepted', 'kade', { card_id: '9999999' });
    const after = fs.existsSync(logPath) ? fs.statSync(logPath).size : 0;
    expect(after).toBe(before);
  });

  // #2652 AC3 — emitChorusEvent retired 2026-05-02. Single emit function in cards.
});
