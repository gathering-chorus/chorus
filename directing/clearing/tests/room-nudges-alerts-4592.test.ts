// @test-type: unit — the router classifies in-process; no I/O, no fixtures on disk.
// @domain: messages
// @card: #4592
// @owner: silas
/**
 * #4592 — nudges between roles and system alerts reach the room, so the room
 * publishes them to Buzz (server.ts publishes every visible message).
 *
 * Jeff, 2026-10-17: "im really not clear what % of messages are in buzz and
 * what are going old path". Two kinds never got there: nudges between roles
 * (the tailer dropped every nudge not addressed to Jeff) and system alerts
 * (84 on 10-17, 0 in Buzz).
 *
 * The alert case is the one a rule can quietly eat: an alert is sent by
 * `system`, and the plumbing sweep hides any non-role message that quotes a
 * path. Alerts quote paths, so the fixture carries one — an alert with no path
 * would pass with or without the alert rule and prove nothing about it.
 */

import { MessageRouter } from '../src/router';

const ALERT = '[alert → wren] log-freshness: /Users/jeffbridwell/Library/Logs/Chorus/embed-worker.log is 134h stale';

const last = (router: MessageRouter) => {
  const all = router.getRecent(10, true);
  return all[all.length - 1];
};

describe('#4592 the room carries every nudge kind', () => {
  test('a system alert is visible, as an alert', () => {
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: ALERT, ts: new Date().toISOString(), type: 'alert' });
    expect(last(router)).toEqual(expect.objectContaining({ type: 'alert', visible: true }));
  });

  test('a nudge between roles is visible, as role-to-role', () => {
    const router = new MessageRouter();
    router.ingest({ from: 'kade', text: '[nudge from kade → silas] internal ping', ts: new Date().toISOString(), type: 'role-to-role' });
    expect(last(router)).toEqual(expect.objectContaining({ type: 'role-to-role', visible: true }));
  });

  // NEGATIVE PROOF (#3734): the same text from `system` WITHOUT the alert type
  // is plumbing and stays hidden. If it went visible, the first test would pass
  // with no alert rule at all, and the sweep that keeps machinery off Jeff's
  // screen would be gone.
  test('negative proof: the same text untyped from system is hidden by the plumbing sweep', () => {
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: ALERT, ts: new Date().toISOString() });
    expect(last(router).visible).toBe(false);
  });
});

// #4592 — a note says which kind of message it is, so Clearing v2 can count
// each kind and its "team nudges" toggle can hide or show them.
import { buildNote } from '../src/buzz-bridge';
import { derivedSigner } from '../src/buzz-signer';

describe('#4592 a published note carries its kind', () => {
  const note = (type: string) => buildNote(
    { from: 'kade', text: 'x', ts: new Date().toISOString(), type, visible: true }, 'team', derivedSigner('bridge', 'test-secret-4592'));

  test('a nudge between roles is tagged kind=nudge, an alert kind=alert', () => {
    expect(note('role-to-role').tags).toContainEqual(['kind', 'nudge']);
    expect(note('alert').tags).toContainEqual(['kind', 'alert']);
  });

  // NEGATIVE PROOF: a reply carries no kind tag, so the toggle never hides one.
  test('negative proof: a reply is not tagged as a nudge or alert', () => {
    expect(note('role-response').tags.some((t) => t[0] === 'kind')).toBe(false);
  });
});

// #4609 — pulse is now the one writer of nudges and alerts to Buzz. v1 still
// shows them (from its spine tailer), but no longer publishes its own copy.
import { publishesToRoom } from '../src/router';
import { ChorusLogTailer } from '../src/tailer';

describe('#4609 v1 shows nudges but leaves writing them to pulse', () => {
  const tail = (role: string, payload: string) => {
    const router = new MessageRouter();
    const tailer = new ChorusLogTailer(router);
    (tailer as unknown as { processLine(l: string): void }).processLine(JSON.stringify({ event: 'nudge.emitted', role, payload }));
    return last(router);
  };

  test('a nudge between roles, an alert and a nudge to Jeff are shown and not republished', () => {
    for (const m of [
      tail('kade', 'from=kade,to=silas,chars=4,content=ping'),
      tail('system', 'from=system,to=wren,chars=9,content=log stale'),
      tail('wren', 'from=wren,to=jeff,chars=5,content=ready'),
    ]) {
      expect(m.visible).toBe(true);
      expect(publishesToRoom(m)).toBe(false);
    }
  });

  // #4628 — since then v1 publishes nothing: replies and Jeff's input have their own one writer too.
  test('a reply and Jeff\'s input are not published by v1 either (#4628)', () => {
    const router = new MessageRouter();
    router.ingest({ from: 'wren', text: 'a reply', ts: new Date().toISOString(), type: 'role-response' });
    expect(publishesToRoom(last(router))).toBe(false);
    router.ingest({ from: 'jeff', text: 'hello', ts: new Date().toISOString(), type: 'jeff-input' });
    expect(publishesToRoom(last(router))).toBe(false);
  });
});
