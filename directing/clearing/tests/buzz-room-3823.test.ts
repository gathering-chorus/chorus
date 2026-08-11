// @test-type: unit — pure identity/attribution logic; injected publish, no relay, brings its own world.
/**
 * #3823 — the room's identity rules, both directions.
 *
 * The claim this card makes to Jeff is that every message in the room is
 * vouched for by a signature that resolves to a Principal. These tests exist to
 * make that claim falsifiable, so each one has a partner showing what failure
 * looks like — a suite that only demonstrates the happy path cannot tell you
 * whether the signature is doing any work.
 */

import { buildRoomIdentity, authorOf, publishToRoom, inboundToClearing } from '../src/buzz-room';
import { derivedSigner, roomSecret } from '../src/buzz-signer';
import type { ClearingMsg, NostrEvent } from '../src/buzz-bridge';

// The test brings its own world (#3528): its own secret, never the live one.
// Pinning the real derivation would put the real secret in the repo, which is
// exactly what the pre-commit scanner stopped — and the property under test is
// that derivation is deterministic and per-actor, which any secret proves.
const TEST_SECRET = 'test-secret-3823';
process.env.BUZZ_ROOM_SECRET = TEST_SECRET;

const identity = buildRoomIdentity();

const msg = (from: string, text = 'hello', visible = true): ClearingMsg => ({
  from, text, ts: '2026-08-11T16:00:00.000Z', type: 'role-response', visible,
});

const noteFrom = (actor: string, content = 'hello', secret?: string): NostrEvent => {
  const signer = secret ? derivedSigner(actor, secret) : derivedSigner(actor);
  const created_at = 1786464000;
  const serialized = JSON.stringify([0, signer.pubkey, created_at, 1, [['t', 'team']], content]);
  const { id, sig } = signer.signEvent(serialized);
  return { id, pubkey: signer.pubkey, created_at, kind: 1, tags: [['t', 'team']], content, sig };
};

describe('#3823 derivation', () => {
  // Pinned against the values Silas and I each computed independently before
  // either of us built anything. If this test ever fails, we have drifted and
  // every actor has two identities — stop, do not debug downstream.
  test.each([
    ['jeff', '234ef89146402369'],
    ['wren', 'ed1196c08b3bc88f'],
    ['silas', '925b41bc53bd67df'],
    ['kade', '64e29eb1cd0dace6'],
  ])('%s derives deterministically from its WebID', (actor, prefix) => {
    expect(derivedSigner(actor, TEST_SECRET).pubkey.slice(0, 16)).toBe(prefix);
  });

  test('NEGATIVE PROOF: no secret means no keys, never a default', () => {
    // A default here would be worse than a crash: every actor would get a
    // second identity derived from a value printed in the source, and the room
    // would look authenticated while being forgeable by anyone with the repo.
    const saved = process.env.BUZZ_ROOM_SECRET;
    delete process.env.BUZZ_ROOM_SECRET;
    try {
      expect(() => roomSecret()).toThrow(/BUZZ_ROOM_SECRET/);
    } finally {
      process.env.BUZZ_ROOM_SECRET = saved;
    }
  });

  test('NEGATIVE PROOF: a different secret derives a different key', () => {
    // Without this, the test above would pass for any implementation that
    // returns a constant, and "derived from the WebID" would be unverified.
    expect(derivedSigner('wren', 'some-other-secret').pubkey).not.toBe(derivedSigner('wren', TEST_SECRET).pubkey);
  });

  test('NEGATIVE PROOF: two actors do not share a key', () => {
    const keys = new Set(['jeff', 'wren', 'silas', 'kade'].map((a) => derivedSigner(a, TEST_SECRET).pubkey));
    expect(keys.size).toBe(4);
  });
});

describe('#3823 inbound attribution', () => {
  test('a note is attributed to whoever signed it', () => {
    const { msg: out, disposition } = inboundToClearing(noteFrom('silas'), identity, new Set());
    expect(disposition).toBe('rendered');
    expect(out?.from).toBe('silas');
  });

  test("Jeff's notes render as his input, not as a role response", () => {
    expect(inboundToClearing(noteFrom('jeff'), identity, new Set()).msg?.type).toBe('jeff-input');
  });

  test('NEGATIVE PROOF: an unbound key is refused, not rendered as a role', () => {
    // The whole claim is that identity comes from the signature. A stranger's
    // note must not appear as anyone — if this renders, the signature is
    // decoration and the room is back to trusting a label.
    const stranger = noteFrom('wren', 'I am totally wren', 'attacker-secret');
    const result = inboundToClearing(stranger, identity, new Set());
    expect(result.msg).toBeNull();
    expect(result.disposition).toBe('unknown-key');
  });

  test('NEGATIVE PROOF: the refusal names which state it is in', () => {
    // A guard that cannot distinguish "unknown key" from "empty message" from
    // "our own echo" tells you nothing when it fires. Today's whole afternoon
    // was lost to a relay saying "must include an h tag" about a membership
    // problem.
    const seen = new Set<string>();
    const ours = noteFrom('wren');
    seen.add(ours.id);
    expect(inboundToClearing(ours, identity, seen).disposition).toBe('own-echo');
    expect(inboundToClearing(noteFrom('wren', '   '), identity, new Set()).disposition).toBe('empty');
    expect(inboundToClearing(noteFrom('wren', 'x', 'nope'), identity, new Set()).disposition).toBe('unknown-key');
  });

  test('our own published note does not come back into the room twice', () => {
    const ours = noteFrom('wren');
    expect(inboundToClearing(ours, identity, new Set([ours.id])).msg).toBeNull();
    // …and the same note from someone else's session DOES render, proving the
    // dedup keys on the id we sent rather than muting Wren generally.
    expect(inboundToClearing(ours, identity, new Set()).msg?.from).toBe('wren');
  });
});

describe('#3823 outbound', () => {
  test('a visible message is published signed by its author', async () => {
    const sent: NostrEvent[] = [];
    const result = await publishToRoom(msg('kade'), {
      topic: 'team', identity, publish: async (ev) => { sent.push(ev); },
    });
    expect(result).toBe('sent');
    expect(sent[0].pubkey).toBe(derivedSigner('kade').pubkey);
    expect(sent[0].tags).toEqual([['t', 'team']]);
  });

  test('the note carries the text alone — the author is the signature', () => {
    // The spike put "[wren] " in the content because one bridge key signed for
    // everyone. Per-actor signing removes the reason; leaving the prefix in
    // would keep teaching readers to trust the body over the key.
    const sent: NostrEvent[] = [];
    return publishToRoom(msg('wren', 'ready when you are'), {
      topic: 'team', identity, publish: async (ev) => { sent.push(ev); },
    }).then(() => {
      expect(sent[0].content).toBe('ready when you are');
      expect(sent[0].content).not.toContain('[wren]');
    });
  });

  test('NEGATIVE PROOF: an actor with no key is refused, never signed by a stand-in', async () => {
    // The tempting fallback is to sign with a default key so the message still
    // gets out. That is exactly how the bridge key made every message look
    // equally authentic.
    const sent: NostrEvent[] = [];
    const result = await publishToRoom(msg('some-stranger'), {
      topic: 'team', identity, publish: async (ev) => { sent.push(ev); },
    });
    expect(result).toBe('no-key');
    expect(sent).toHaveLength(0);
  });

  test('hidden messages stay out of the room', async () => {
    const sent: NostrEvent[] = [];
    const result = await publishToRoom(msg('wren', 'internal', false), {
      topic: 'team', identity, publish: async (ev) => { sent.push(ev); },
    });
    expect(result).toBe('not-visible');
    expect(sent).toHaveLength(0);
  });
});

describe('#3823 identity table', () => {
  test('authorOf resolves a known key and refuses an unknown one', () => {
    expect(authorOf({ pubkey: derivedSigner('jeff').pubkey }, identity)).toBe('jeff');
    expect(authorOf({ pubkey: 'ff'.repeat(32) }, identity)).toBeNull();
  });
});
