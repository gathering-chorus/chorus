// @test-type: unit — the router classifies in-process; no I/O, no fixtures on disk.
/**
 * #3823 — the classifier's fallback must not swallow role messages.
 *
 * Jeff, 2026-08-11: "i got nothing in clearing pretty certain messages are not
 * always reaching". They were reaching. The router filed them hidden because
 * they matched no classification rule and the fallback defaulted to
 * role-to-role/invisible — so the room showed his half of the conversation and
 * little of ours.
 *
 * NEGATIVE PROOF (#3734) is the point of this file. It is not enough to assert
 * "an unclassified role message renders" — a test that passes because the
 * message happens to match some OTHER rule proves nothing about the default.
 * So each case here pins BOTH halves: the message reaches the fallback (no
 * earlier rule claims it), and the fallback renders it.
 */

import { MessageRouter } from '../src/router';

/** A shape no classification rule matches: no prefix, no keyword, no type. */
const UNCLASSIFIED = 'The derivation matches on all four keys, so we have not drifted.';

/** Text that DOES match an earlier rule, used to prove the fixture discriminates. */
const NUDGE = '[nudge from wren | 2026-08-11 10:22 Boston] taking the buzz thread';

const ingest = (from: string, text: string, type?: string) => {
  const router = new MessageRouter();
  router.ingest({ from, text, ts: new Date().toISOString(), ...(type ? { type } : {}) });
  const all = router.getRecent(10, true);
  return all[all.length - 1];
};

describe('#3823 classifier default — unrecognized role messages are visible', () => {
  test('a role message matching no rule renders instead of disappearing', () => {
    const msg = ingest('wren', UNCLASSIFIED);
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('role-response');
  });

  test.each(['wren', 'silas', 'kade'])('holds for %s, not just the sender we tested with', (role) => {
    expect(ingest(role, UNCLASSIFIED).visible).toBe(true);
  });

  // ── The negative proofs ────────────────────────────────────────────────
  // Each one fails if the fixture stops exercising the thing it claims to.

  test('NEGATIVE PROOF: the fixture text really does reach the fallback', () => {
    // If some earlier rule ever starts claiming this text, the test above would
    // pass for the wrong reason — it would be proving that rule, not the
    // default. A rule-matched message is typed by its rule; the fallback types
    // it 'role-response'. Pin the discriminator: the nudge text must NOT come
    // back as a fallback-typed message.
    const claimed = ingest('wren', NUDGE);
    expect(claimed.type).toBe('role-to-role');
    expect(claimed.visible).toBe(false);
  });

  test('NEGATIVE PROOF: hiding still happens when a rule names a reason', () => {
    // The fix must not turn the router into "show everything". Probes, bridge
    // echo and role-to-role nudges are hidden BY RULE and must stay hidden —
    // otherwise this card trades an empty room for an unreadable one.
    expect(ingest('probe', 'heartbeat', 'probe').visible).toBe(false);
    expect(ingest('wren', '[bridge] subscriber echo').visible).toBe(false);
    expect(ingest('wren', '[progress] 4/10 files').visible).toBe(false);
  });

  test('NEGATIVE PROOF: non-role senders still default to hidden', () => {
    // Plumbing has to earn its way onto the screen. If this passes for a role
    // AND a non-role alike, the change was "show everything" and the role
    // check is doing no work.
    const plumbing = ingest('some-worker', UNCLASSIFIED);
    expect(plumbing.visible).toBe(false);
    expect(plumbing.type).toBe('role-to-role');
  });

  test('NEGATIVE PROOF: Buzz-shaped messages survive the fallback', () => {
    // The shape Silas's relay will deliver: role text with no prefix and no
    // type the router has ever seen. Before this fix a working relay would
    // have rendered an empty room and we would have debugged the relay.
    const fromRelay = ingest('silas', 'relay is up, group id is literally team');
    expect(fromRelay.visible).toBe(true);
  });
});
