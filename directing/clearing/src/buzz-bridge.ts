// @test-type: unit — injects signer/publish/flag; no live relay, no real key, brings its own world.
/**
 * #3674 — Clearing → Buzz mirror bridge (SPIKE, flag-off by default).
 *
 * One-way tap on the Clearing message fan-out: a VISIBLE room message becomes a
 * signed Nostr kind:9 group-chat event published to the relay's channel. Relay→
 * Clearing write-back is explicitly OUT of scope for the spike.
 *
 * Identity discipline (Jeff's steer, 2026-07-25): the bridge signs as its OWN
 * identity — `principal-bridge`'s derived nostr key (#3618 KeyRegistryEntry,
 * keyType=nostr-secp256k1, keyId=BUZZ_BRIDGE_NOSTR_KEY), key material at
 * ~/.chorus/buzz/bridge.key, never embedded. It NEVER forges a role/human pubkey;
 * the true author is preserved in the event content (`[from] text`). The relay
 * allowlist is a PROJECTION of our Principal allow-set — no ad-hoc identity.
 *
 * Crypto boundary: the `NostrSigner` is INJECTED (secp256k1 Schnorr / sha256 live
 * in buzz-signer.ts, a vetted @noble adapter). This module is pure assembly +
 * gating — hermetically unit-tested with a stub signer; the real signature is
 * proven live when the relay accepts the event (a bad sig is rejected at the door).
 */

export interface ClearingMsg {
  from: string;
  text: string;
  ts: string;
  type: string;
  visible: boolean;
}

export interface NostrEvent {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
}

/** The injected crypto boundary: hex pubkey + a hash-and-sign over the canonical
 *  NIP-01 serialization. buzz-signer.ts provides the @noble-backed live impl. */
export interface NostrSigner {
  pubkey: string;
  /** Given the canonical serialization, return the event id (sha256) + Schnorr sig. */
  signEvent(serialized: string): { id: string; sig: string };
}

/** NIP-01 canonical serialization: [0, pubkey, created_at, kind, tags, content]. */
function serialize(pubkey: string, created_at: number, kind: number, tags: string[][], content: string): string {
  return JSON.stringify([0, pubkey, created_at, kind, tags, content]);
}

/**
 * #3823 — build a signed kind:1 note carrying a Clearing message, topic-tagged
 * rather than group-scoped.
 *
 * Why kind:1 and not the kind:9 above: this relay build refuses NIP-29 group
 * writes from non-members, and its own add-member call is refused with "must
 * include an h tag" while carrying that very tag. A refusal naming the wrong
 * state cost an hour today; rather than fight it, the test-drive uses plain
 * notes with a topic tag. Access control is a real question and a later one —
 * it is not what Jeff is trying to learn by talking in a room.
 *
 * The content is the message text ALONE. No "[wren] " prefix: the author is the
 * signature now, and a name in the body invites reading the body to find out
 * who spoke, which is the habit this card exists to end.
 */
export function buildNote(msg: ClearingMsg, topic: string, signer: NostrSigner): NostrEvent {
  if (!signer.pubkey) throw new Error('buzz-bridge: no signing key — refusing to build an unsigned event');
  const pubkey = signer.pubkey;
  const parsed = Date.parse(msg.ts);
  // A relay rejects a stale or absent timestamp, and an unparseable ts would
  // send NaN — serialized as null, failing at the door with a complaint about
  // the signature rather than about the clock.
  const created_at = Number.isFinite(parsed) ? Math.floor(parsed / 1000) : Math.floor(Date.now() / 1000);
  const tags: string[][] = [['t', topic]];
  const { id, sig } = signer.signEvent(serialize(pubkey, created_at, 1, tags, msg.text));
  return { id, pubkey, created_at, kind: 1, tags, content: msg.text, sig };
}

// #3893 — BuzzBridgeDeps / BuzzBridge / buildKind9 DELETED with the #3696
// one-way mirror they served. Two publishers on one relay meant a message could
// land twice under two authors. The room (buzz-room.ts) is the only publisher.
