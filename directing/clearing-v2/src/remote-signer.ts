// @domain: messages
// #4652 reopen 2 — Jeff picked A2 (10-20 11:53): a signing service (a NIP-46 "bunker")
// holds a copy of his Buzz app key, and Clearing v2 asks it to sign each post he types,
// so the Buzz app shows the post as Jeff and the room still never holds his key.
// The room holds only its own client key for talking to the bunker. Spec: NIP-46.
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes, randomBytes } from '@noble/hashes/utils';
import { conversationKey, decrypt, encrypt } from './nip44';
import { buildEventId, serializeEvent, verifyNostrEvent } from './identity';
import { NostrEvent } from './types';

export const NIP46_KIND = 24133;

/** How the signer reaches the relay: publish an event, and subscribe to events for a filter. */
export interface SignerTransport {
  publish(event: NostrEvent): Promise<{ ok: boolean; reason: string }>;
  subscribe(filter: Record<string, unknown>, onEvent: (event: NostrEvent) => void): () => void;
}

export interface UnsignedEvent { kind: number; tags: string[][]; content: string; created_at: number }

export type SignResult = { ok: true; event: NostrEvent } | { ok: false; reason: string };

export interface RemoteSignerOptions {
  /** The room's own client key (hex), used only to talk to the bunker. */
  clientSecret: string;
  /** The bunker's pubkey, from the bunker:// address. */
  bunkerPubkey: string;
  /** The person the bunker signs as (Jeff's app key); anything else is refused. */
  userPubkey: string;
  transport: SignerTransport;
  timeoutMs?: number;
}

export class RemoteSigner {
  private readonly clientPubkey: string;
  private readonly convKey: Uint8Array;
  private readonly timeoutMs: number;

  constructor(private readonly o: RemoteSignerOptions) {
    this.clientPubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(o.clientSecret)));
    this.convKey = conversationKey(o.clientSecret, o.bunkerPubkey);
    this.timeoutMs = o.timeoutMs ?? 8000;
  }

  /** Ask the bunker to sign `unsigned` as the user. Never throws; a refusal says why. */
  async signEvent(unsigned: UnsignedEvent): Promise<SignResult> {
    const id = bytesToHex(randomBytes(16));
    const request = this.wrap(JSON.stringify({ id, method: 'sign_event', params: [JSON.stringify(unsigned)] }));
    return new Promise<SignResult>((resolve) => {
      let done = false;
      const finish = (r: SignResult) => { if (done) return; done = true; clearTimeout(timer); stop(); resolve(r); };
      const timer = setTimeout(() => finish({ ok: false, reason: `signing service did not answer within ${this.timeoutMs} ms` }), this.timeoutMs);
      const stop = this.o.transport.subscribe({ kinds: [NIP46_KIND], authors: [this.o.bunkerPubkey], '#p': [this.clientPubkey] }, (e) => {
        const reply = this.unwrap(e);
        if (!reply || reply.id !== id) return;
        finish(this.check(reply, unsigned));
      });
      void this.o.transport.publish(request).then((r) => { if (!r.ok) finish({ ok: false, reason: `request not stored: ${r.reason}` }); });
    });
  }

  private wrap(plaintext: string): NostrEvent {
    const created_at = Math.floor(Date.now() / 1000);
    const tags = [['p', this.o.bunkerPubkey]];
    const content = encrypt(plaintext, this.convKey);
    const id = buildEventId(serializeEvent(this.clientPubkey, created_at, NIP46_KIND, tags, content));
    return { id, pubkey: this.clientPubkey, created_at, kind: NIP46_KIND, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(this.o.clientSecret))) };
  }

  private unwrap(e: NostrEvent): { id?: unknown; result?: unknown; error?: unknown } | null {
    if (e.kind !== NIP46_KIND || e.pubkey !== this.o.bunkerPubkey || !verifyNostrEvent(e)) return null;
    try { return JSON.parse(decrypt(e.content, this.convKey)); } catch { return null; }
  }

  /** The bunker's answer is trusted only if it is the exact event asked for, signed by the user. */
  private check(reply: { result?: unknown; error?: unknown }, asked: UnsignedEvent): SignResult {
    if (typeof reply.error === 'string' && reply.error) return { ok: false, reason: `signing service refused: ${reply.error}` };
    let signed: NostrEvent;
    try { signed = JSON.parse(String(reply.result)); } catch { return { ok: false, reason: 'signing service sent no event' }; }
    if (signed.pubkey !== this.o.userPubkey) return { ok: false, reason: 'signed by a key that is not Jeff\'s' };
    if (!verifyNostrEvent(signed)) return { ok: false, reason: 'signature does not verify' };
    const same = signed.kind === asked.kind && signed.content === asked.content && signed.created_at === asked.created_at
      && JSON.stringify(signed.tags) === JSON.stringify(asked.tags);
    if (!same) return { ok: false, reason: 'signing service returned a different event than the one asked for' };
    return { ok: true, event: signed };
  }
}
