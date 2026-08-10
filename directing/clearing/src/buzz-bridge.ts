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
 * Build a signed kind:9 event mirroring a Clearing message. Pure assembly;
 * hashing + signing delegated to the injected signer. Deterministic over
 * (msg, channel, signer).
 */
export function buildKind9(msg: ClearingMsg, channel: string, signer: NostrSigner): NostrEvent {
  // `!signer` is dead per the type. The guard that earns its keep is the empty
  // pubkey — the type cannot rule that out, and it would produce an event signed
  // by nobody. Narrowed rather than deleted: refusing to build an unsigned event
  // is the whole point of this line.
  if (!signer.pubkey) throw new Error('buzz-bridge: no signing key — refusing to build an unsigned event');
  const pubkey = signer.pubkey;
  const created_at = Math.floor(Date.parse(msg.ts) / 1000);
  const kind = 9;
  const tags: string[][] = [['h', channel]];
  // author preserved in content — the bridge signs as itself, never spoofs the sender
  const content = `[${msg.from}] ${msg.text}`;
  const { id, sig } = signer.signEvent(serialize(pubkey, created_at, kind, tags, content));
  return { id, pubkey, created_at, kind, tags, content, sig };
}

export interface BuzzBridgeDeps {
  enabled: boolean;
  channel: string;
  signer: NostrSigner | null;
  /** Publish a signed event to the relay (WebSocket ["EVENT", ev]). Injected. */
  publish: (ev: NostrEvent) => Promise<void>;
  /** Optional structured logger for the best-effort path. */
  log?: (level: 'info' | 'error', event: string, fields: Record<string, unknown>) => void;
}

export class BuzzBridge {
  constructor(private readonly d: BuzzBridgeDeps) {}

  /** Mirror one message. Throws on misconfiguration (missing signer) so setup fails loud. */
  async onMessage(msg: ClearingMsg): Promise<void> {
    if (!this.d.enabled) return;          // flag-off: dark, safe to ship
    if (!msg.visible) return;             // mirror only what the room sees
    if (!this.d.signer) throw new Error('buzz-bridge: enabled but no signing key present');
    const ev = buildKind9(msg, this.d.channel, this.d.signer);
    await this.d.publish(ev);
    this.d.log?.('info', 'buzz.bridge.mirrored', { from: msg.from, id: ev.id });
  }

  /** The wrapper the Clearing listener uses: best-effort, NEVER throws into the
   *  room's hot path. A relay outage must not break the Clearing. */
  async onMessageSafe(msg: ClearingMsg): Promise<void> {
    try {
      await this.onMessage(msg);
    } catch (err) {
      this.d.log?.('error', 'buzz.bridge.failed', { reason: err instanceof Error ? err.message : String(err) });
    }
  }
}
